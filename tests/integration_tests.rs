use newsatoshi_commons::{
    sha256, CheckedWorkProof, CommonsLedger, CovenantRule, HardwareDeviceConfig,
    HardwareTelemetrySigner, JsonRpcAnchorClient, MerkleLamportKeypair, MerkleSignError,
    StreamingChannel, DEFAULT_TREE_HEIGHT, LARGE_TREE_HEIGHT, OP_VERIFY_COVENANT_STARK_HEX,
    PROTOCOL_VERSION,
};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;
use std::time::Duration;

#[test]
fn merkle_tree_64_and_1024_leaves_verify_and_reject_tampering() {
    // 64-leaf tree (height = 6)
    let mut tree64 = MerkleLamportKeypair::new_64(sha256(b"oracle_seed_64"));
    assert_eq!(tree64.height(), DEFAULT_TREE_HEIGHT);
    assert_eq!(tree64.total_capacity(), 64);
    assert_eq!(tree64.remaining_signatures(), 64);
    let root64 = tree64.root();

    for i in 0..5u32 {
        let msg = sha256(format!("telemetry_msg_{i}").as_bytes());
        let sig = tree64.sign_next(msg).expect("signing should succeed");
        assert_eq!(sig.leaf_index, i);
        assert_eq!(sig.auth_path.len(), 6);
        assert!(sig.verify(root64, msg));

        // Tampered message must fail
        let wrong_msg = sha256(b"tampered_msg");
        assert!(!sig.verify(root64, wrong_msg));

        // Wrong root must fail
        assert!(!sig.verify(sha256(b"wrong_root"), msg));

        // Tampered auth path sibling must fail
        let mut tampered_path_sig = sig.clone();
        tampered_path_sig.auth_path[0][0] ^= 0xff;
        assert!(!tampered_path_sig.verify(root64, msg));

        // Tampered leaf index must fail
        let mut tampered_idx_sig = sig.clone();
        tampered_idx_sig.leaf_index = (i + 1) % 64;
        assert!(!tampered_idx_sig.verify(root64, msg));
    }
    assert_eq!(tree64.remaining_signatures(), 59);

    // 1,024-leaf tree (height = 10)
    let mut tree1024 = MerkleLamportKeypair::new_1024(sha256(b"hardware_seed_1024"));
    assert_eq!(tree1024.height(), LARGE_TREE_HEIGHT);
    assert_eq!(tree1024.total_capacity(), 1024);
    let root1024 = tree1024.root();

    // Sign leaf #0 and leaf #1023 (far edges of the 1,024-leaf tree)
    let msg0 = sha256(b"edge_leaf_0");
    let sig0 = tree1024.sign_next(msg0).unwrap();
    assert!(sig0.verify(root1024, msg0));
    assert_eq!(sig0.auth_path.len(), 10);

    tree1024.skip_leaves(1022).unwrap();
    let msg_last = sha256(b"edge_leaf_1023");
    let sig_last = tree1024.sign_next(msg_last).unwrap();
    assert_eq!(sig_last.leaf_index, 1023);
    assert!(sig_last.verify(root1024, msg_last));
    assert_eq!(tree1024.remaining_signatures(), 0);
}

#[test]
fn merkle_tree_exhaustion_prevents_key_reuse() {
    let mut tree = MerkleLamportKeypair::from_seed(sha256(b"small_tree"), 3).unwrap();
    let root = tree.root();
    assert_eq!(tree.total_capacity(), 8);

    for i in 0..8u32 {
        let msg = sha256(format!("packet_{i}").as_bytes());
        let sig = tree.sign_next(msg).expect("leaf within capacity");
        assert_eq!(sig.leaf_index, i);
        assert!(sig.verify(root, msg));
    }

    let err = tree
        .sign_next(sha256(b"ninth_packet"))
        .expect_err("9th signature on 8-leaf tree must fail");
    assert_eq!(err, MerkleSignError::TreeExhausted { capacity: 8 });
}

#[test]
fn covenant_continuity_rejects_strip_and_rule_substitution() {
    let mut ledger = CommonsLedger::new();
    let mut oracle = MerkleLamportKeypair::new_64(sha256(b"oracle_continuity"));
    let hw = MerkleLamportKeypair::new_64(sha256(b"hw_continuity"));

    let rule = CovenantRule {
        max_bounty_payout: 20_000,
        reinvest_bps: 1_000,
        min_soil_health_score: 80,
        authorized_deed_oracle_pk: oracle.root(),
        authorized_hardware_pk: hw.root(),
        ..CovenantRule::default()
    };
    let treasury_ref = ledger.mint_genesis_covenant(100_000, &rule);

    let deed = sha256(b"PARCEL_1_DEED");
    let sig = oracle
        .sign_next(CheckedWorkProof::deed_msg(1, deed))
        .unwrap();

    // 1. Attempt to strip covenant on change output (`None`)
    let strip_err = ledger
        .spend_covenant(
            treasury_ref,
            &rule,
            40_000,
            "SELLER",
            None,
            CheckedWorkProof::merkle_land_deed(1, deed, sig.clone()),
        )
        .unwrap_err();
    assert!(strip_err.contains("Recursive covenant continuity violated"));

    // 2. Attempt to swap in a weaker forged covenant rule hash on change output
    let forged_rule = CovenantRule {
        reinvest_bps: 0,
        min_soil_health_score: 0,
        ..rule.clone()
    };
    let swap_err = ledger
        .spend_covenant(
            treasury_ref,
            &rule,
            40_000,
            "SELLER",
            Some(forged_rule.rule_hash()),
            CheckedWorkProof::merkle_land_deed(1, deed, sig.clone()),
        )
        .unwrap_err();
    assert!(swap_err.contains("Recursive covenant continuity violated"));

    // Treasury and oracle watermark must remain untouched after rejected attempts
    assert_eq!(ledger.total_covenant_balance(&rule), 100_000);
    assert_eq!(ledger.next_merkle_leaf_watermark(oracle.root()), 0);

    // 3. Exact spend (change == 0) is allowed with `proposed_change_covenant: None`
    let (_, change_ref) = ledger
        .spend_covenant(
            treasury_ref,
            &rule,
            100_000,
            "SELLER",
            None,
            CheckedWorkProof::merkle_land_deed(1, deed, sig),
        )
        .expect("exact spend with zero change should succeed");
    assert!(change_ref.is_none());
    assert_eq!(ledger.total_covenant_balance(&rule), 0);
}

#[test]
fn merkle_leaf_replay_and_stale_index_blocked_by_ledger() {
    let mut ledger = CommonsLedger::new();
    let oracle = MerkleLamportKeypair::new_64(sha256(b"oracle_replay"));
    let mut hw = MerkleLamportKeypair::new_64(sha256(b"hw_replay"));
    let rogue_hw = MerkleLamportKeypair::new_64(sha256(b"rogue_hw"));

    let rule = CovenantRule {
        max_bounty_payout: 10_000,
        reinvest_bps: 1_500,
        min_soil_health_score: 80,
        authorized_deed_oracle_pk: oracle.root(),
        authorized_hardware_pk: hw.root(),
        ..CovenantRule::default()
    };
    let treasury_ref = ledger.mint_genesis_covenant(50_000, &rule);

    // Spend with hardware leaf #0
    let sig0 = hw
        .sign_next(CheckedWorkProof::energy_msg(10, 5_000, 90))
        .unwrap();
    let (_, change_ref) = ledger
        .spend_covenant(
            treasury_ref,
            &rule,
            5_000,
            "BOT",
            Some(rule.rule_hash()),
            CheckedWorkProof::merkle_energy_deployment(10, 5_000, 90, sig0.clone()),
        )
        .expect("leaf #0 should succeed");
    let treasury_ref = change_ref.unwrap();
    assert_eq!(ledger.next_merkle_leaf_watermark(hw.root()), 1);

    // 1. Exact replay of leaf #0 MUST fail
    let replay_err = ledger
        .spend_covenant(
            treasury_ref,
            &rule,
            5_000,
            "BOT",
            Some(rule.rule_hash()),
            CheckedWorkProof::merkle_energy_deployment(10, 5_000, 90, sig0),
        )
        .unwrap_err();
    assert!(replay_err.contains("already spent"));

    // 2. Re-signing a DIFFERENT message on spent leaf #0 MUST fail
    let forged_reuse = hw
        .sign_at_index_unchecked(0, CheckedWorkProof::energy_msg(10, 9_999, 95))
        .unwrap();
    let reuse_err = ledger
        .spend_covenant(
            treasury_ref,
            &rule,
            5_000,
            "BOT",
            Some(rule.rule_hash()),
            CheckedWorkProof::merkle_energy_deployment(10, 9_999, 95, forged_reuse),
        )
        .unwrap_err();
    assert!(reuse_err.contains("already spent"));

    // 3. Unauthorized rogue Merkle root MUST fail
    let rogue_sig = rogue_hw
        .sign_at_index_unchecked(1, CheckedWorkProof::energy_msg(10, 5_000, 90))
        .unwrap();
    let rogue_err = ledger
        .spend_covenant(
            treasury_ref,
            &rule,
            5_000,
            "ATTACKER",
            Some(rule.rule_hash()),
            CheckedWorkProof::merkle_energy_deployment(10, 5_000, 90, rogue_sig),
        )
        .unwrap_err();
    assert!(rogue_err.contains("Invalid Merkleized hardware telemetry signature"));

    // 4. Skip leaves #1 and #2, submit leaf #3 -> succeeds and sets watermark to 4
    hw.skip_leaves(2).unwrap();
    let sig3 = hw
        .sign_next(CheckedWorkProof::energy_msg(10, 6_000, 91))
        .unwrap();
    assert_eq!(sig3.leaf_index, 3);
    let (_, change_ref) = ledger
        .spend_covenant(
            treasury_ref,
            &rule,
            5_000,
            "BOT",
            Some(rule.rule_hash()),
            CheckedWorkProof::merkle_energy_deployment(10, 6_000, 91, sig3),
        )
        .expect("leaf #3 should succeed");
    let treasury_ref = change_ref.unwrap();
    assert_eq!(ledger.next_merkle_leaf_watermark(hw.root()), 4);

    // 5. Backfilling skipped stale leaf #1 MUST fail monotonic watermark check
    let stale_sig1 = hw
        .sign_at_index_unchecked(1, CheckedWorkProof::energy_msg(10, 6_000, 91))
        .unwrap();
    let stale_err = ledger
        .spend_covenant(
            treasury_ref,
            &rule,
            5_000,
            "BOT",
            Some(rule.rule_hash()),
            CheckedWorkProof::merkle_energy_deployment(10, 6_000, 91, stale_sig1),
        )
        .unwrap_err();
    assert!(stale_err.contains("monotonic watermark"));

    // 6. Leaf #4 continues smoothly
    let sig4 = hw
        .sign_next(CheckedWorkProof::energy_msg(10, 7_000, 93))
        .unwrap();
    assert_eq!(sig4.leaf_index, 4);
    ledger
        .spend_covenant(
            treasury_ref,
            &rule,
            5_000,
            "BOT",
            Some(rule.rule_hash()),
            CheckedWorkProof::merkle_energy_deployment(10, 7_000, 93, sig4),
        )
        .expect("leaf #4 should succeed");
    assert_eq!(ledger.total_covenant_balance(&rule), 35_000);
    assert_eq!(ledger.total_merkle_leaves_consumed, 3);
}

#[test]
fn low_soil_score_bounty_cap_and_zero_kwh_rejected() {
    let mut ledger = CommonsLedger::new();
    let oracle = MerkleLamportKeypair::new_64(sha256(b"oracle_eco"));
    let mut hw = MerkleLamportKeypair::new_64(sha256(b"hw_eco"));

    let rule = CovenantRule {
        max_bounty_payout: 10_000,
        reinvest_bps: 1_500,
        min_soil_health_score: 80,
        authorized_deed_oracle_pk: oracle.root(),
        authorized_hardware_pk: hw.root(),
        ..CovenantRule::default()
    };
    let treasury_ref = ledger.mint_genesis_covenant(50_000, &rule);

    // 1. Low soil health score (79 < 80) rejected
    let low_soil_sig = hw
        .sign_next(CheckedWorkProof::energy_msg(404, 10_000, 79))
        .unwrap();
    let err = ledger
        .spend_covenant(
            treasury_ref,
            &rule,
            5_000,
            "BOT",
            Some(rule.rule_hash()),
            CheckedWorkProof::merkle_energy_deployment(404, 10_000, 79, low_soil_sig),
        )
        .unwrap_err();
    assert!(err.contains("soil score below floor"));

    // 2. Payout above max_bounty_payout (10_001 > 10_000) rejected
    let over_cap_sig = hw
        .sign_next(CheckedWorkProof::energy_msg(404, 10_000, 90))
        .unwrap();
    let err = ledger
        .spend_covenant(
            treasury_ref,
            &rule,
            10_001,
            "BOT",
            Some(rule.rule_hash()),
            CheckedWorkProof::merkle_energy_deployment(404, 10_000, 90, over_cap_sig),
        )
        .unwrap_err();
    assert!(err.contains("max_bounty_payout"));

    // 3. Zero kWh rejected
    let zero_kwh_sig = hw
        .sign_next(CheckedWorkProof::energy_msg(404, 0, 95))
        .unwrap();
    let err = ledger
        .spend_covenant(
            treasury_ref,
            &rule,
            5_000,
            "BOT",
            Some(rule.rule_hash()),
            CheckedWorkProof::merkle_energy_deployment(404, 0, 95, zero_kwh_sig),
        )
        .unwrap_err();
    assert!(err.contains("Zero physical output"));

    // Nothing was mutated
    assert_eq!(ledger.total_covenant_balance(&rule), 50_000);
    assert_eq!(ledger.next_merkle_leaf_watermark(hw.root()), 0);
}

#[test]
fn streaming_channel_compounding_into_parcel_2() {
    let mut ledger = CommonsLedger::new();
    let mut oracle = MerkleLamportKeypair::new_64(sha256(b"oracle_parcel2"));
    let mut hw = MerkleLamportKeypair::new_64(sha256(b"hw_parcel2"));

    let rule = CovenantRule {
        max_bounty_payout: 15_000,
        reinvest_bps: 1_500, // 15%
        min_soil_health_score: 80,
        authorized_deed_oracle_pk: oracle.root(),
        authorized_hardware_pk: hw.root(),
        ..CovenantRule::default()
    };
    let treasury_ref = ledger.mint_genesis_covenant(100_000, &rule);

    // Acquire Parcel #1 (40,000 LOVE) using Oracle Leaf #0
    let deed1 = sha256(b"PARCEL_101_DEED");
    let sig_deed1 = oracle
        .sign_next(CheckedWorkProof::deed_msg(101, deed1))
        .unwrap();
    let (_, change_ref) = ledger
        .spend_covenant(
            treasury_ref,
            &rule,
            40_000,
            "PARCEL_101_SELLER",
            Some(rule.rule_hash()),
            CheckedWorkProof::merkle_land_deed(101, deed1, sig_deed1),
        )
        .unwrap();
    let mut treasury_ref = change_ref.unwrap();

    // Deploy hardware on Parcel #1 (3 bounties x 10,000 = 30,000 LOVE -> 30,000 left)
    for kwh in [10_000u64, 12_000, 14_000] {
        let sig = hw
            .sign_next(CheckedWorkProof::energy_msg(101, kwh, 92))
            .unwrap();
        let (_, next_ref) = ledger
            .spend_covenant(
                treasury_ref,
                &rule,
                10_000,
                "ROBOT_FLEET",
                Some(rule.rule_hash()),
                CheckedWorkProof::merkle_energy_deployment(101, kwh, 92, sig),
            )
            .unwrap();
        treasury_ref = next_ref.unwrap();
    }
    assert_eq!(ledger.total_covenant_balance(&rule), 30_000);

    // Settle streaming channel with invalid preimage -> must fail
    let (ch1, chain1) =
        StreamingChannel::create_with_secret_chain(sha256(b"ch1_seed"), 1_000, 200);
    assert!(ledger
        .settle_streaming_channel(&ch1, 500, sha256(b"fake_preimage"), &rule, "MAINT_BOT")
        .is_err());

    // Settle two valid streaming channels: each gross 100k -> 15k reinvested x 2 = +30k
    let (reinvest1, op1) = ledger
        .settle_streaming_channel(&ch1, 500, chain1[500], &rule, "MAINT_BOT")
        .unwrap();
    assert_eq!((reinvest1, op1), (15_000, 85_000));

    let (ch2, chain2) =
        StreamingChannel::create_with_secret_chain(sha256(b"ch2_seed"), 1_000, 200);
    let (reinvest2, op2) = ledger
        .settle_streaming_channel(&ch2, 500, chain2[500], &rule, "MAINT_BOT")
        .unwrap();
    assert_eq!((reinvest2, op2), (15_000, 85_000));

    // Treasury compounded to 60,000 LOVE across 3 UTXOs
    let utxo_refs = ledger.covenant_utxo_refs(&rule);
    assert_eq!(utxo_refs.len(), 3);
    assert_eq!(ledger.total_covenant_balance(&rule), 60_000);

    // Consolidate covenant UTXOs into one UTXO preserving exact rule_hash
    let consolidated = ledger.consolidate_covenant_utxos(&utxo_refs, &rule).unwrap();
    assert_eq!(ledger.covenant_utxo_refs(&rule).len(), 1);
    assert_eq!(ledger.total_covenant_balance(&rule), 60_000);

    // Acquire Parcel #2 (45,000 LOVE) using Oracle Leaf #1 under the SAME burned-in root!
    let deed2 = sha256(b"PARCEL_102_ADJACENT_DEED");
    let sig_deed2 = oracle
        .sign_next(CheckedWorkProof::deed_msg(102, deed2))
        .unwrap();
    assert_eq!(sig_deed2.leaf_index, 1);

    let (_, final_change) = ledger
        .spend_covenant(
            consolidated,
            &rule,
            45_000,
            "PARCEL_102_SELLER",
            Some(rule.rule_hash()),
            CheckedWorkProof::merkle_land_deed(102, deed2, sig_deed2),
        )
        .expect("Parcel #2 acquisition should succeed from compounded treasury");

    assert!(final_change.is_some());
    assert_eq!(ledger.acquired_parcels, vec![101, 102]);
    assert_eq!(ledger.total_kwh_verified, 36_000);
    assert_eq!(ledger.total_covenant_balance(&rule), 15_000);
    assert_eq!(ledger.next_merkle_leaf_watermark(oracle.root()), 2);
}

#[test]
fn hardware_telemetry_signer_to_ledger_and_deterministic_rpc_anchor() {
    let config = HardwareDeviceConfig::from_passphrase(
        "greenhouse-robot-07",
        707,
        DEFAULT_TREE_HEIGHT,
        "tee-silicon-fuse-707",
    );
    let mut signer = HardwareTelemetrySigner::new(config).unwrap();
    let oracle = MerkleLamportKeypair::new_64(sha256(b"oracle_707"));

    let rule = CovenantRule {
        max_bounty_payout: 10_000,
        reinvest_bps: 1_000,
        min_soil_health_score: 85,
        authorized_deed_oracle_pk: oracle.root(),
        authorized_hardware_pk: signer.root(),
        ..CovenantRule::default()
    };

    let mut ledger = CommonsLedger::new();
    let mut treasury_ref = ledger.mint_genesis_covenant(80_000, &rule);

    for i in 0..5u64 {
        let pkt = signer
            .sign_telemetry(8_000 + i * 500, 88 + (i as u16), 1_795_200_000 + i * 3600)
            .unwrap();
        assert!(pkt.verify());
        let summary = pkt.to_json_summary();
        assert_eq!(summary.leaf_index, i as u32);
        assert_eq!(summary.auth_path_hashes.len(), DEFAULT_TREE_HEIGHT as usize);

        let (_, next_ref) = ledger
            .spend_covenant(
                treasury_ref,
                &rule,
                5_000,
                "GREENHOUSE_OPERATOR",
                Some(rule.rule_hash()),
                pkt.to_checked_work_proof(),
            )
            .unwrap();
        treasury_ref = next_ref.unwrap();
    }

    assert_eq!(ledger.total_covenant_balance(&rule), 55_000);
    assert_eq!(ledger.total_kwh_verified, 45_000);
    assert_eq!(signer.remaining_leaves(), 59);

    let commitment_a = ledger.build_anchor_commitment(&rule);
    let commitment_b = ledger.build_anchor_commitment(&rule);
    assert_eq!(commitment_a, commitment_b);
    assert_eq!(commitment_a.protocol_version, PROTOCOL_VERSION);
    assert!(commitment_a.op_return_script_hex.starts_with("6a20"));
    assert!(commitment_a
        .stark_witness_commitment_hex
        .ends_with(OP_VERIFY_COVENANT_STARK_HEX));

    let client = JsonRpcAnchorClient::testnet_default();
    let json_a = client.anchor_payload_json(&commitment_a, 42);
    let json_b = client.anchor_payload_json(&commitment_b, 42);
    assert_eq!(json_a, json_b);

    let parsed: serde_json::Value = serde_json::from_str(&json_a).unwrap();
    assert_eq!(parsed["jsonrpc"], "2.0");
    assert_eq!(parsed["id"], 42);
    assert_eq!(parsed["method"], "newsatoshi_anchorCovenantRoot");
    assert_eq!(parsed["params"]["network"], "testnet");
    assert_eq!(
        parsed["params"]["commitment"]["anchor_hash"],
        commitment_a.anchor_hash
    );

    let receipt = client.submit_dry_run(&commitment_a, 42);
    assert_eq!(receipt.broadcast_status, "unbroadcast");
    assert!(receipt.l1_txid.is_none());
    assert_eq!(receipt.anchor_hash.len(), 64);
}

#[test]
fn live_http_anchor_posts_to_local_mock_rpc_node() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral localhost port");
    let addr = listener.local_addr().unwrap();

    let server_handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept client");
        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
        let mut buf = [0u8; 4096];
        let n = stream.read(&mut buf).expect("read HTTP request");
        let req_str = String::from_utf8_lossy(&buf[..n]).to_string();

        let resp_body = r#"{"jsonrpc":"2.0","id":7,"result":{"accepted":true}}"#;
        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            resp_body.len(),
            resp_body
        );
        stream.write_all(resp.as_bytes()).expect("write response");
        req_str
    });

    let ledger = CommonsLedger::new();
    let rule = CovenantRule {
        max_bounty_payout: 10_000,
        reinvest_bps: 1_000,
        min_soil_health_score: 80,
        authorized_deed_oracle_pk: sha256(b"oracle"),
        authorized_hardware_pk: sha256(b"hw"),
        ..CovenantRule::default()
    };
    let commitment = ledger.build_anchor_commitment(&rule);
    let client = JsonRpcAnchorClient::new(format!("http://{addr}/rpc"), "regtest");

    let response = client
        .submit_live_http(&commitment, 7, Duration::from_secs(2))
        .expect("live HTTP JSON-RPC POST should succeed against mock node");
    assert!(response.contains("200 OK"));
    assert!(response.contains("\"accepted\":true"));

    let received = server_handle.join().unwrap();
    assert!(received.starts_with("POST /rpc HTTP/1.1"));
    assert!(received.contains("\"method\":\"newsatoshi_anchorCovenantRoot\""));
    assert!(received.contains(&commitment.anchor_hash));
}

#[test]
fn slh_dsa_fips_205_keygen_sign_verify_and_ledger_nonce_replay_guard() {
    use newsatoshi_commons::{SlhDsaKeypair, SlhDsaPubKey, SlhDsaSig, SLH_DSA_PK_BYTES};

    // Deterministic keygen & hex round-trip
    let oracle_slh = SlhDsaKeypair::from_seed(sha256(b"slh_deed_oracle_seed"));
    let oracle_slh_repeat = SlhDsaKeypair::from_seed(sha256(b"slh_deed_oracle_seed"));
    assert_eq!(oracle_slh.pk, oracle_slh_repeat.pk);
    assert_eq!(oracle_slh.pk.to_bytes().len(), SLH_DSA_PK_BYTES);

    let pk_roundtrip = SlhDsaPubKey::from_hex(&oracle_slh.pk.to_hex()).unwrap();
    assert_eq!(pk_roundtrip, oracle_slh.pk);

    let hw_slh = SlhDsaKeypair::from_seed(sha256(b"slh_hardware_seed"));
    let rogue_slh = SlhDsaKeypair::from_seed(sha256(b"slh_rogue_attacker"));

    // Sign/verify & tamper checks
    let msg = sha256(b"fips_205_test_message");
    let sig = hw_slh.sign_hash(msg);
    assert_eq!(sig.len(), 17_088); // SLH-DSA-SHA2-128f exact FIPS 205 signature length
    let sig_roundtrip = SlhDsaSig::from_hex(&sig.to_hex()).unwrap();
    assert!(hw_slh.pk.verify_hash(msg, &sig_roundtrip));
    assert!(!hw_slh.pk.verify_hash(sha256(b"tampered"), &sig_roundtrip));
    assert!(!rogue_slh.pk.verify_hash(msg, &sig_roundtrip));

    // Dual-authorized CovenantRule (XMSS + SLH-DSA)
    let oracle_xmss = MerkleLamportKeypair::new_64(sha256(b"xmss_oracle"));
    let hw_xmss = MerkleLamportKeypair::new_64(sha256(b"xmss_hw"));
    let rule = CovenantRule {
        max_bounty_payout: 15_000,
        reinvest_bps: 1_500,
        min_soil_health_score: 80,
        authorized_deed_oracle_pk: oracle_xmss.root(),
        authorized_hardware_pk: hw_xmss.root(),
        authorized_deed_oracle_slh_pk: Some(oracle_slh.pk.commitment()),
        authorized_hardware_slh_pk: Some(hw_slh.pk.commitment()),
    };

    let mut ledger = CommonsLedger::new();
    let treasury_ref = ledger.mint_genesis_covenant(100_000, &rule);

    // 1. Acquire Parcel #404 using NIST FIPS 205 SLH-DSA (nonce 0)
    let deed_hash = sha256(b"SLH_DEED_PARCEL_404");
    let deed_msg = CheckedWorkProof::deed_msg_with_nonce(404, deed_hash, 0);
    let deed_sig = oracle_slh.sign_hash(deed_msg);
    let (_, change_ref) = ledger
        .spend_covenant(
            treasury_ref,
            &rule,
            40_000,
            "PARCEL_404_SELLER",
            Some(rule.rule_hash()),
            CheckedWorkProof::slh_dsa_land_deed(
                404,
                deed_hash,
                0,
                oracle_slh.pk.clone(),
                deed_sig.clone(),
            ),
        )
        .expect("valid SLH-DSA deed spend should succeed");
    let treasury_ref = change_ref.unwrap();
    assert_eq!(ledger.total_covenant_balance(&rule), 60_000);
    assert_eq!(ledger.total_slh_dsa_signatures_verified, 1);

    // 2. Replaying SLH-DSA deed nonce 0 MUST be blocked
    let replay_err = ledger
        .spend_covenant(
            treasury_ref,
            &rule,
            10_000,
            "ATTACKER",
            Some(rule.rule_hash()),
            CheckedWorkProof::slh_dsa_land_deed(404, deed_hash, 0, oracle_slh.pk.clone(), deed_sig),
        )
        .unwrap_err();
    assert!(replay_err.contains("replay blocked"));

    // 3. Unauthorized rogue SLH-DSA hardware key MUST be rejected
    let rogue_msg = CheckedWorkProof::energy_msg_with_nonce(404, 12_000, 92, 0);
    let rogue_sig = rogue_slh.sign_hash(rogue_msg);
    let rogue_err = ledger
        .spend_covenant(
            treasury_ref,
            &rule,
            10_000,
            "ROGUE",
            Some(rule.rule_hash()),
            CheckedWorkProof::slh_dsa_energy_deployment(
                404,
                12_000,
                92,
                0,
                rogue_slh.pk.clone(),
                rogue_sig,
            ),
        )
        .unwrap_err();
    assert!(rogue_err.contains("Unauthorized SLH-DSA"));

    // 4. Valid SLH-DSA hardware telemetry (nonce 0) succeeds
    let hw_msg = CheckedWorkProof::energy_msg_with_nonce(404, 12_500, 93, 0);
    let hw_sig = hw_slh.sign_hash(hw_msg);
    let (_, change_ref) = ledger
        .spend_covenant(
            treasury_ref,
            &rule,
            10_000,
            "SOLAR_BOT",
            Some(rule.rule_hash()),
            CheckedWorkProof::slh_dsa_energy_deployment(
                404,
                12_500,
                93,
                0,
                hw_slh.pk.clone(),
                hw_sig,
            ),
        )
        .expect("valid SLH-DSA telemetry spend should succeed");
    assert!(change_ref.is_some());
    assert_eq!(ledger.total_covenant_balance(&rule), 50_000);
    assert_eq!(ledger.total_kwh_verified, 12_500);
    assert_eq!(ledger.total_slh_dsa_signatures_verified, 2);
}

#[test]
fn winterfell_stark_proves_verifies_and_rejects_tampering() {
    use newsatoshi_commons::{
        prove_covenant_transition, verify_covenant_stark, CovenantPublicInputs, CovenantTxKind,
    };

    let rule_hash = hex::encode(sha256(b"STARK_COVENANT_RULE_V2_1"));
    let prev_root = hex::encode(sha256(b"STARK_PREV_ROOT"));
    let new_root = hex::encode(sha256(b"STARK_NEW_ROOT"));

    // 1. Telemetry Bounty STARK (value conservation + bounty cap + ecological soil floor + rule continuity)
    let telemetry_inputs = CovenantPublicInputs {
        tx_kind: CovenantTxKind::TelemetryBounty,
        label: "Parcel #404 Solar Telemetry 12,500 kWh".to_string(),
        tx_nonce: 2,
        covenant_rule_hash: rule_hash.clone(),
        change_covenant_rule_hash: rule_hash.clone(),
        prev_state_root: prev_root.clone(),
        new_state_root: new_root.clone(),
        input_amount: 60_000,
        payout_amount: 10_000,
        change_amount: 50_000,
        max_bounty_payout: 25_000,
        reinvest_bps: 1_500,
        min_soil_health_score: 80,
        soil_health_score: 92,
    };
    let artifact = prove_covenant_transition(telemetry_inputs.clone())
        .expect("valid telemetry transition must produce verified Winterfell STARK");
    assert!(artifact.verified);
    assert!(artifact.proof_bytes_len > 1_000);
    assert!(artifact.security_bits >= 95);

    let recheck = verify_covenant_stark(&artifact).expect("re-verification must parse");
    assert!(recheck.valid);
    println!(
        "WINTERFELL STARK TELEMETRY | bytes={} | prove={}ms | verify={}ms | security={}-bit",
        artifact.proof_bytes_len, artifact.prove_ms, recheck.verify_ms, artifact.security_bits
    );

    // Tampered payout_amount in public inputs MUST fail winterfell::verify
    let mut tampered_payout = artifact.clone();
    tampered_payout.public_inputs.payout_amount = 10_001;
    let res = verify_covenant_stark(&tampered_payout).unwrap();
    assert!(!res.valid, "tampered payout_amount must fail verification");

    // Tampered change_covenant_rule_hash MUST fail winterfell::verify
    let mut tampered_rule = artifact.clone();
    tampered_rule.public_inputs.change_covenant_rule_hash =
        hex::encode(sha256(b"FORGED_WEAKER_RULE"));
    let res = verify_covenant_stark(&tampered_rule).unwrap();
    assert!(
        !res.valid,
        "tampered change_covenant_rule_hash must fail verification"
    );

    // Tampered new_state_root MUST fail Fiat-Shamir public coin verification
    let mut tampered_root = artifact.clone();
    tampered_root.public_inputs.new_state_root = hex::encode(sha256(b"FORGED_ROOT"));
    let res = verify_covenant_stark(&tampered_root).unwrap();
    assert!(!res.valid, "tampered new_state_root must fail verification");

    // Corrupted proof bytes MUST fail deserialization or cryptographic verification
    let mut corrupted = artifact.clone();
    let mut bytes = hex::decode(&corrupted.proof_hex).unwrap();
    let mid = bytes.len() / 2;
    bytes[mid] ^= 0x5a;
    corrupted.proof_hex = hex::encode(bytes);
    if let Ok(r) = verify_covenant_stark(&corrupted) {
        assert!(!r.valid, "corrupted STARK proof bytes must not verify");
    }

    // 2. Land Deed STARK
    let deed_inputs = CovenantPublicInputs {
        tx_kind: CovenantTxKind::LandDeed,
        label: "Acquire Parcel #404".to_string(),
        tx_nonce: 1,
        covenant_rule_hash: rule_hash.clone(),
        change_covenant_rule_hash: rule_hash.clone(),
        prev_state_root: prev_root.clone(),
        new_state_root: new_root.clone(),
        input_amount: 100_000,
        payout_amount: 40_000,
        change_amount: 60_000,
        max_bounty_payout: 25_000,
        reinvest_bps: 1_500,
        min_soil_health_score: 80,
        soil_health_score: 0,
    };
    let deed_art = prove_covenant_transition(deed_inputs).expect("deed STARK must prove & verify");
    assert!(deed_art.verified);

    // 3. Streaming Settlement STARK (proving exact 15% reinvest_bps split: 100k gross -> 15k reinvest + 85k operator)
    let stream_inputs = CovenantPublicInputs {
        tx_kind: CovenantTxKind::StreamingSettlement,
        label: "PayWord Stream Settlement".to_string(),
        tx_nonce: 3,
        covenant_rule_hash: rule_hash.clone(),
        change_covenant_rule_hash: rule_hash.clone(),
        prev_state_root: prev_root.clone(),
        new_state_root: new_root.clone(),
        input_amount: 100_000,
        payout_amount: 85_000,
        change_amount: 15_000,
        max_bounty_payout: 25_000,
        reinvest_bps: 1_500,
        min_soil_health_score: 80,
        soil_health_score: 0,
    };
    let stream_art =
        prove_covenant_transition(stream_inputs).expect("stream STARK must prove & verify");
    assert!(stream_art.verified);

    // 4. Invalid witnesses MUST be rejected before proving
    let mut bad_cap = telemetry_inputs.clone();
    bad_cap.payout_amount = 30_000;
    bad_cap.change_amount = 30_000;
    assert!(prove_covenant_transition(bad_cap).is_err());

    let mut bad_soil = telemetry_inputs.clone();
    bad_soil.soil_health_score = 72;
    assert!(prove_covenant_transition(bad_soil).is_err());

    let mut bad_conservation = telemetry_inputs.clone();
    bad_conservation.change_amount = 50_001;
    assert!(prove_covenant_transition(bad_conservation).is_err());

    let mut bad_continuity = telemetry_inputs;
    bad_continuity.change_covenant_rule_hash = hex::encode(sha256(b"STRIPPED"));
    assert!(prove_covenant_transition(bad_continuity).is_err());
}

#[test]
fn persistent_storage_atomic_roundtrip_and_legal_schedule_export() {
    use newsatoshi_commons::{
        load_state, save_state_atomic, CommonsEngine, AUXPOW_CHAIN_ID, MAINNET_GENESIS_HASH,
        TESTNET_GENESIS_HASH,
    };

    let tmp_dir = std::env::temp_dir().join(format!(
        "newsatoshi_commons_storage_test_{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&tmp_dir);
    let state_path = tmp_dir.join("commons-ledger.json");

    let mut engine = CommonsEngine::new_empty();
    assert_eq!(engine.ledger.total_covenant_balance(&engine.rule), 0);
    engine.record_telemetry_series_point("Snapshot Test");

    // Legal schedule export check
    let schedule = engine.export_legal_schedule();
    assert_eq!(schedule.protocol_version, PROTOCOL_VERSION);
    assert_eq!(
        schedule.covenant_rule_hash,
        hex::encode(engine.rule.rule_hash())
    );
    assert_eq!(schedule.l1_anchors.mainnet_genesis, MAINNET_GENESIS_HASH);
    assert_eq!(schedule.l1_anchors.testnet_genesis, TESTNET_GENESIS_HASH);
    assert_eq!(schedule.l1_anchors.auxpow_chainid, AUXPOW_CHAIN_ID);
    assert_eq!(schedule.keys.deed_oracle_slh_dsa_pk_hex.len(), 64);
    assert_eq!(schedule.keys.hardware_slh_dsa_pk_hex.len(), 64);

    // Atomic save & reload roundtrip
    let before = engine.to_persisted();
    save_state_atomic(&state_path, &before).expect("atomic save must succeed");
    let loaded = load_state(&state_path).expect("load must succeed");
    assert_eq!(before, loaded);

    let restored = CommonsEngine::from_persisted(&loaded).expect("engine restore must succeed");
    assert_eq!(restored.rule.rule_hash(), engine.rule.rule_hash());
    assert_eq!(restored.ledger.state_root, engine.ledger.state_root);
    assert_eq!(
        restored.signing_keys.as_ref().unwrap().hardware_xmss.root(),
        engine.signing_keys.as_ref().unwrap().hardware_xmss.root()
    );
    assert_eq!(
        restored.signing_keys.as_ref().unwrap().hardware_slh.pk,
        engine.signing_keys.as_ref().unwrap().hardware_slh.pk
    );

    let _ = std::fs::remove_dir_all(&tmp_dir);
}

#[test]
fn seed_status_parsing_l1_tip_binding_and_offline_fallback() {
    use newsatoshi_commons::{
        SeedStatusClient, AUXPOW_CHAIN_ID, MAINNET_GENESIS_HASH, TESTNET_GENESIS_HASH,
    };

    let sample_live_json = r#"{
        "network":"testnet",
        "proofs":"test",
        "fee":300000,
        "version":"0.1.0",
        "genesis":"00000fd814d823a0bc64a84135868402998cff81e28a85b2bede5f1125d0a394",
        "height":21182,
        "tip":"0000007565f3524f29b603f5be08673faf14e25576f5d0c2b18e321b55053dec",
        "tip_time":1795323190,
        "now":1795323289,
        "bits":"1e00a8d1",
        "difficulty":0.005923504494759007,
        "chainwork":"0000000000000000000000000000000000000000000000000000007a52955b74",
        "peers":4,
        "known_peers":1,
        "best_header":21182,
        "mempool":0,
        "issued":105910000000000,
        "notes":21199,
        "nullifiers":13,
        "locks":0,
        "conditions":0,
        "auxpow_chainid":20051,
        "recent":[
            {"height":21182,"hash":"0000007565f3524f29b603f5be08673faf14e25576f5d0c2b18e321b55053dec","time":1795323190,"txs":1,"bytes":207,"merge_mined":false}
        ]
    }"#;

    let parsed = SeedStatusClient::parse_status_json(sample_live_json).unwrap();
    assert_eq!(parsed.network, "testnet");
    assert_eq!(parsed.height, 21182);
    assert_eq!(parsed.auxpow_chainid, AUXPOW_CHAIN_ID);
    assert_eq!(parsed.recent.len(), 1);
    assert_eq!(parsed.genesis, TESTNET_GENESIS_HASH);

    let ledger = CommonsLedger::new();
    let rule = CovenantRule {
        max_bounty_payout: 25_000,
        reinvest_bps: 1_500,
        min_soil_health_score: 80,
        authorized_deed_oracle_pk: sha256(b"o"),
        authorized_hardware_pk: sha256(b"h"),
        ..CovenantRule::default()
    };
    let bound = ledger
        .build_anchor_commitment(&rule)
        .with_seed_status(&parsed);
    assert_eq!(bound.l1_height, Some(21182));
    assert_eq!(bound.l1_tip_hash.as_deref(), Some(parsed.tip.as_str()));
    assert_eq!(bound.auxpow_chainid, Some(20051));
    assert!(bound
        .cli_lock_command
        .contains("newsatoshi --net testnet lock 0.01 --hash "));
    assert!(bound.cli_send_memo_command.contains("--memo "));
    assert!(bound
        .stark_witness_commitment_hex
        .ends_with(OP_VERIFY_COVENANT_STARK_HEX));

    // Honest disconnected state against closed local port (no fabricated status fields)
    let offline_client = SeedStatusClient::new("http://127.0.0.1:1/status", Duration::from_millis(300));
    let snap = offline_client.fetch_snapshot();
    assert!(!snap.live);
    assert!(snap.status.is_none());
    assert_eq!(snap.mainnet_info.genesis, MAINNET_GENESIS_HASH);
    assert!(snap.error.is_some());
}

#[test]
fn http_api_server_full_abundance_loop_red_team_external_hw_and_persistence() {
    use newsatoshi_commons::server::{spawn_server, ServerConfig};
    use newsatoshi_commons::storage::{default_hardware_slh_seed, default_hardware_xmss_seed};
    use newsatoshi_commons::{ExternalTelemetryPacket, SlhDsaKeypair, MAINNET_GENESIS_HASH};
    use serde_json::{json, Value};

    // 1. Spin up tiny local mock seed server serving live-formatted /status JSON
    let mock_seed = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let seed_addr = mock_seed.server_addr().to_ip().unwrap().to_string();
    let seed_stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let seed_stop_clone = std::sync::Arc::clone(&seed_stop);
    let seed_thread = thread::spawn(move || {
        let body = r#"{"network":"testnet","proofs":"test","fee":300000,"version":"0.1.0","genesis":"00000fd814d823a0bc64a84135868402998cff81e28a85b2bede5f1125d0a394","height":21182,"tip":"0000007565f3524f29b603f5be08673faf14e25576f5d0c2b18e321b55053dec","tip_time":1795323190,"now":1795323289,"bits":"1e00a8d1","difficulty":0.0059,"chainwork":"0000","peers":4,"known_peers":1,"best_header":21182,"mempool":0,"issued":105910000000000,"notes":21199,"nullifiers":13,"locks":0,"conditions":0,"auxpow_chainid":20051,"recent":[{"height":21182,"hash":"0000007565f3524f29b603f5be08673faf14e25576f5d0c2b18e321b55053dec","time":1795323190,"txs":1,"bytes":207,"merge_mined":false}]}"#;
        while !seed_stop_clone.load(std::sync::atomic::Ordering::SeqCst) {
            if let Ok(Some(req)) = mock_seed.recv_timeout(Duration::from_millis(50)) {
                let _ = req.respond(tiny_http::Response::from_string(body));
            }
        }
    });

    let tmp_dir = std::env::temp_dir().join(format!(
        "newsatoshi_commons_http_test_{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&tmp_dir);
    let state_file = tmp_dir.join("commons-ledger.json");

    let cfg = ServerConfig {
        host: "127.0.0.1".to_string(),
        port: 0,
        state_file: state_file.clone(),
        seed_url: format!("http://{seed_addr}/status"),
        mainnet_seed_url: None,
        static_dir: None,
        admin_token: None,
        verify_only: false,
        external_public_keys: None,
    };

    let srv = spawn_server(cfg.clone()).expect("server must start");
    let base = srv.base_url();

    let get_json = |path: &str| -> Value {
        let mut r = ureq::get(&format!("{base}{path}")).call().unwrap();
        serde_json::from_str(&r.body_mut().read_to_string().unwrap()).unwrap()
    };
    let post_json = |path: &str, body: Value| -> Value {
        let mut r = ureq::post(&format!("{base}{path}"))
            .header("Content-Type", "application/json")
            .send(body.to_string().as_str())
            .unwrap();
        serde_json::from_str(&r.body_mut().read_to_string().unwrap()).unwrap()
    };

    // GET /api/health
    let health = get_json("/api/health");
    assert_eq!(health["ok"], true);
    assert_eq!(health["protocol_version"], PROTOCOL_VERSION);

    // GET /api/seed
    let seed = get_json("/api/seed");
    assert_eq!(seed["live"], true);
    assert_eq!(seed["status"]["height"], 21182);
    assert_eq!(seed["status"]["auxpow_chainid"], 20051);

    // GET /api/state — MUST start at genuine 0 LOVE, empty ledger, null verified_love_per_kwh
    let state0 = get_json("/api/state");
    assert_eq!(state0["ledger_summary"]["balance_love"], 0);
    assert_eq!(state0["ledger_summary"]["parcels_acquired_count"], 0);
    assert_eq!(state0["ledger_summary"]["total_kwh_verified"], 0);
    assert!(state0["ledger_summary"]["verified_love_per_kwh"].is_null());
    assert!(state0["telemetry_series"].as_array().unwrap().is_empty());
    assert_eq!(state0["keys"]["hardware_remaining_leaves"], 64);

    // Record explicit L1 funding deposit (test fixture isolated in integration test)
    let test_l1_txid = hex::encode(sha256(b"test_l1_funding_tx_1"));
    let fund = post_json(
        "/api/actions/record-funding",
        json!({
            "l1_network": "testnet",
            "l1_txid": test_l1_txid,
            "amount_love": 100_000,
            "note": "Integration test funding fixture"
        }),
    );
    assert_eq!(fund["ok"], true);
    assert_eq!(fund["state"]["ledger_summary"]["balance_love"], 100_000);
    assert_eq!(fund["state"]["ledger_summary"]["funding_deposits_count"], 1);

    // Duplicate funding txid must be rejected
    let dup_fund = post_json(
        "/api/actions/record-funding",
        json!({
            "l1_network": "testnet",
            "l1_txid": test_l1_txid,
            "amount_love": 100_000,
            "note": "Duplicate must fail"
        }),
    );
    assert_eq!(dup_fund["ok"], false);

    // POST /api/actions/acquire-parcel with NIST FIPS 205 SLH-DSA
    let acq = post_json(
        "/api/actions/acquire-parcel",
        json!({
            "parcel_id": 404,
            "acres": 10.0,
            "price_love": 40_000,
            "deed_label": "Parcel #404 — Test Fixture",
            "signature_scheme": "slh_dsa"
        }),
    );
    assert_eq!(acq["ok"], true);
    assert_eq!(acq["state"]["ledger_summary"]["balance_love"], 60_000);
    assert_eq!(acq["stark_proof"]["verified"], true);
    let first_proof_id = acq["stark_proof"]["proof_id"].as_str().unwrap().to_string();

    // POST /api/actions/telemetry — all 4 red-team attacks MUST return HTTP 200 + blocked: true
    for atk in ["strip_covenant", "replay_leaf", "low_soil", "over_cap"] {
        let resp = post_json(
            "/api/actions/telemetry",
            json!({
                "parcel_id": 404,
                "device_id": "test-meter-01",
                "kwh_delivered": 12_500,
                "soil_health_score": 92,
                "payout_love": 10_000,
                "signature_scheme": "xmss",
                "simulate_attack": atk
            }),
        );
        assert_eq!(resp["ok"], false, "attack {atk} must not succeed");
        assert_eq!(resp["blocked"], true, "attack {atk} must be marked blocked");
        assert_eq!(
            resp["state"]["ledger_summary"]["balance_love"], 60_000,
            "attack {atk} must not mutate treasury"
        );
    }

    // Valid telemetry via XMSS leaf #0
    let tel_xmss = post_json(
        "/api/actions/telemetry",
        json!({
            "parcel_id": 404,
            "device_id": "test-meter-01",
            "kwh_delivered": 12_500,
            "soil_health_score": 92,
            "payout_love": 10_000,
            "signature_scheme": "xmss",
            "simulate_attack": "none"
        }),
    );
    assert_eq!(tel_xmss["ok"], true);
    assert_eq!(tel_xmss["blocked"], false);
    assert_eq!(tel_xmss["state"]["ledger_summary"]["balance_love"], 50_000);
    assert_eq!(
        tel_xmss["state"]["ledger_summary"]["total_kwh_verified"],
        12_500
    );
    // Real observed LOVE / kWh ratio: 10_000 / 12_500 = 0.8
    assert_eq!(
        tel_xmss["state"]["ledger_summary"]["verified_love_per_kwh"]
            .as_f64()
            .unwrap(),
        0.8
    );

    // External hardware wire-signed packet via POST /api/telemetry (XMSS leaf #1 + SLH-DSA nonce #0)
    let ext_xmss_kp =
        MerkleLamportKeypair::from_seed_at_index(default_hardware_xmss_seed(), DEFAULT_TREE_HEIGHT, 1)
            .unwrap();
    let mut ext_signer =
        HardwareTelemetrySigner::from_keypair("pi-gateway-01", 404, ext_xmss_kp);
    let ext_signed = ext_signer.sign_telemetry(8_000, 94, 1_795_323_500).unwrap();
    let ext_wire = ext_signed.to_external_wire_packet(5_000);
    let ext_resp = post_json("/api/telemetry", serde_json::to_value(&ext_wire).unwrap());
    assert_eq!(ext_resp["ok"], true);
    assert_eq!(ext_resp["state"]["ledger_summary"]["balance_love"], 45_000);

    let ext_slh_kp = SlhDsaKeypair::from_seed(default_hardware_slh_seed());
    let ext_slh_pkt = ExternalTelemetryPacket::sign_slh_dsa(
        &ext_slh_kp,
        "esp32-s3-sensor-01",
        404,
        7_000,
        95,
        5_000,
        0,
        1_795_323_600,
    );
    let ext_slh_resp = post_json("/api/telemetry", serde_json::to_value(&ext_slh_pkt).unwrap());
    assert_eq!(ext_slh_resp["ok"], true);
    assert_eq!(ext_slh_resp["state"]["ledger_summary"]["balance_love"], 40_000);

    // POST /api/actions/stream-settle (500 * 200 = 100,000 gross -> +15,000 reinvested => 55,000 LOVE)
    let stream = post_json(
        "/api/actions/stream-settle",
        json!({ "ticks": 500, "unit_price": 200, "operator_tag": "MICROGRID_BOT" }),
    );
    assert_eq!(stream["ok"], true);
    assert_eq!(stream["gross_love"], 100_000);
    assert_eq!(stream["reinvested_love"], 15_000);
    assert_eq!(stream["operator_paid_love"], 85_000);
    assert_eq!(stream["state"]["ledger_summary"]["balance_love"], 55_000);
    assert_eq!(stream["stark_proof"]["verified"], true);

    // POST /api/actions/anchor (testnet + confirm-anchor + mainnet)
    let anc_test = post_json("/api/actions/anchor", json!({ "network": "testnet" }));
    assert_eq!(anc_test["ok"], true);
    assert_eq!(anc_test["receipt"]["l1_height"], 21182);
    assert_eq!(anc_test["receipt"]["broadcast_status"], "unbroadcast");
    assert!(anc_test["receipt"]["l1_txid"].is_null());
    assert!(anc_test["receipt"]["cli_lock_command"]
        .as_str()
        .unwrap()
        .contains("newsatoshi --net testnet lock 0.01 --hash "));

    let anchor_hash = anc_test["receipt"]["anchor_hash"]
        .as_str()
        .unwrap()
        .to_string();
    let confirmed_l1_txid = hex::encode(sha256(b"confirmed_testnet_lock_tx"));
    let confirm = post_json(
        "/api/actions/confirm-anchor",
        json!({ "anchor_hash": anchor_hash, "l1_txid": confirmed_l1_txid }),
    );
    assert_eq!(confirm["ok"], true);
    assert_eq!(confirm["receipt"]["broadcast_status"], "broadcast");
    assert_eq!(confirm["receipt"]["l1_txid"], confirmed_l1_txid);

    let anc_main = post_json("/api/actions/anchor", json!({ "network": "mainnet" }));
    assert_eq!(anc_main["ok"], true);
    assert_eq!(anc_main["receipt"]["l1_height"], 0);
    assert_eq!(anc_main["receipt"]["l1_genesis"], MAINNET_GENESIS_HASH);
    assert_eq!(anc_main["receipt"]["broadcast_status"], "unbroadcast");

    // POST /api/actions/verify-stark (re-verify stored Winterfell proof bytes!)
    let vstark = post_json(
        "/api/actions/verify-stark",
        json!({ "proof_id": first_proof_id }),
    );
    assert_eq!(vstark["ok"], true);
    assert_eq!(vstark["valid"], true);
    assert!(vstark["security_bits"].as_u64().unwrap() >= 95);

    // Restart server on same state_file -> state MUST persist intact!
    let saved_root = anc_main["state"]["ledger_summary"]["state_root"]
        .as_str()
        .unwrap()
        .to_string();
    srv.stop();

    let srv2 = spawn_server(cfg).expect("restarted server must load persisted state");
    let base2 = srv2.base_url();
    let mut r = ureq::get(&format!("{base2}/api/state")).call().unwrap();
    let persisted: Value = serde_json::from_str(&r.body_mut().read_to_string().unwrap()).unwrap();
    assert_eq!(persisted["ledger_summary"]["balance_love"], 55_000);
    assert_eq!(persisted["ledger_summary"]["state_root"], saved_root);
    assert_eq!(persisted["ledger_summary"]["parcels_acquired_count"], 1);
    assert_eq!(persisted["stark_proofs"].as_array().unwrap().len(), 5);

    // POST /api/actions/reset -> honest empty reset (0 LOVE)
    let mut r_reset = ureq::post(&format!("{base2}/api/actions/reset"))
        .header("Content-Type", "application/json")
        .send("{}")
        .unwrap();
    let reset_val: Value =
        serde_json::from_str(&r_reset.body_mut().read_to_string().unwrap()).unwrap();
    assert_eq!(reset_val["ok"], true);
    assert_eq!(reset_val["state"]["ledger_summary"]["balance_love"], 0);
    assert_eq!(
        reset_val["state"]["ledger_summary"]["parcels_acquired_count"],
        0
    );

    srv2.stop();
    seed_stop.store(true, std::sync::atomic::Ordering::SeqCst);
    let _ = seed_thread.join();
    let _ = std::fs::remove_dir_all(&tmp_dir);
}

#[test]
fn winterfell_stark_boundary_zero_slacks_and_exact_edge_rejections() {
    use newsatoshi_commons::{
        prove_covenant_transition, verify_covenant_stark, CovenantPublicInputs, CovenantTxKind,
    };

    let rule_hash = hex::encode(sha256(b"BOUNDARY_RULE"));
    let prev_root = hex::encode(sha256(b"BOUNDARY_PREV"));
    let new_root = hex::encode(sha256(b"BOUNDARY_NEW"));

    // Exact boundary: payout == max_bounty_payout (slack=0), soil == min_soil (slack=0), change == 0
    let exact_edge = CovenantPublicInputs {
        tx_kind: CovenantTxKind::TelemetryBounty,
        label: "Exact Zero-Slack Boundary".to_string(),
        tx_nonce: 9,
        covenant_rule_hash: rule_hash.clone(),
        change_covenant_rule_hash: rule_hash.clone(),
        prev_state_root: prev_root.clone(),
        new_state_root: new_root.clone(),
        input_amount: 25_000,
        payout_amount: 25_000,
        change_amount: 0,
        max_bounty_payout: 25_000,
        reinvest_bps: 1_500,
        min_soil_health_score: 80,
        soil_health_score: 80,
    };
    let art = prove_covenant_transition(exact_edge.clone())
        .expect("exact zero-slack boundary must prove cleanly");
    assert!(verify_covenant_stark(&art).unwrap().valid);

    // Off-by-one above bounty cap MUST fail
    let mut over_by_one = exact_edge.clone();
    over_by_one.input_amount = 25_001;
    over_by_one.payout_amount = 25_001;
    assert!(prove_covenant_transition(over_by_one).is_err());

    // Off-by-one below soil floor MUST fail
    let mut soil_below_by_one = exact_edge;
    soil_below_by_one.soil_health_score = 79;
    assert!(prove_covenant_transition(soil_below_by_one).is_err());
}

#[test]
fn hw_signer_binary_provision_sign_stdin_and_daemon_end_to_end() {
    use newsatoshi_commons::server::{spawn_server, ServerConfig};
    use serde_json::Value;
    use std::process::{Command, Stdio};

    let hw_bin = env!("CARGO_BIN_EXE_newsatoshi-hw-signer");
    let tmp_dir = std::env::temp_dir().join(format!(
        "newsatoshi_hw_signer_e2e_{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&tmp_dir);
    std::fs::create_dir_all(&tmp_dir).unwrap();
    let keyfile = tmp_dir.join("device-key.json");
    let state_file = tmp_dir.join("server-state.json");

    // 1. Provision keyfile matching default engine hardware roots
    let prov = Command::new(hw_bin)
        .args([
            "provision",
            "--keyfile",
            keyfile.to_str().unwrap(),
            "--device-id",
            "rpi5-solar-inverter-01",
            "--parcel",
            "404",
            "--default-engine-seed",
            "--json",
        ])
        .output()
        .expect("run provision");
    assert!(
        prov.status.success(),
        "provision failed: {}",
        String::from_utf8_lossy(&prov.stderr)
    );
    let prov_json: Value = serde_json::from_slice(&prov.stdout).unwrap();
    assert_eq!(prov_json["ok"], true);
    assert_eq!(prov_json["tree_capacity"], 64);

    // 2. Sign #1 (XMSS leaf #0)
    let sign1 = Command::new(hw_bin)
        .args([
            "sign",
            "--keyfile",
            keyfile.to_str().unwrap(),
            "--kwh",
            "11000",
            "--soil",
            "91",
            "--payout",
            "5000",
        ])
        .output()
        .expect("run sign 1");
    assert!(sign1.status.success());
    let pkt1: Value = serde_json::from_slice(&sign1.stdout).unwrap();
    assert_eq!(pkt1["merkle_sig"]["leaf_index"], 0);

    // 3. Sign #2 via --stdin-json (XMSS leaf #1 — persisted monotonically across process restarts!)
    let mut child = Command::new(hw_bin)
        .args([
            "sign",
            "--keyfile",
            keyfile.to_str().unwrap(),
            "--stdin-json",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn sign stdin");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(br#"{"kwh_delivered":12000,"soil_health_score":93,"payout_love":6000}"#)
        .unwrap();
    let sign2 = child.wait_with_output().unwrap();
    assert!(sign2.status.success());
    let pkt2: Value = serde_json::from_slice(&sign2.stdout).unwrap();
    assert_eq!(pkt2["merkle_sig"]["leaf_index"], 1);
    assert_eq!(pkt2["payout_love"], 6000);

    // 4. Sign #3 with NIST FIPS 205 SLH-DSA
    let sign3 = Command::new(hw_bin)
        .args([
            "sign",
            "--keyfile",
            keyfile.to_str().unwrap(),
            "--scheme",
            "slh_dsa",
            "--kwh",
            "13000",
            "--soil",
            "95",
            "--payout",
            "5000",
        ])
        .output()
        .expect("run sign slh_dsa");
    assert!(sign3.status.success());
    let pkt3: Value = serde_json::from_slice(&sign3.stdout).unwrap();
    assert_eq!(pkt3["signature_scheme"], "slh_dsa");
    assert_eq!(pkt3["slh_dsa_nonce"], 0);

    // 5. Start live HTTP server, fund via record-funding, and run `newsatoshi-hw-signer daemon --reading-file`
    let srv = spawn_server(ServerConfig {
        host: "127.0.0.1".to_string(),
        port: 0,
        state_file,
        seed_url: "http://127.0.0.1:1/status".to_string(),
        mainnet_seed_url: None,
        static_dir: None,
        admin_token: None,
        verify_only: false,
        external_public_keys: None,
    })
    .unwrap();
    let fund_txid = hex::encode(sha256(b"hw_signer_test_funding"));
    let _ = ureq::post(&format!("{}/api/actions/record-funding", srv.base_url()))
        .header("Content-Type", "application/json")
        .send(
            serde_json::json!({
                "l1_network": "testnet",
                "l1_txid": fund_txid,
                "amount_love": 100_000,
                "note": "Test fixture funding for hw-signer integration test"
            })
            .to_string()
            .as_str(),
        )
        .unwrap();

    let reading_file = tmp_dir.join("meter-readings.jsonl");
    std::fs::write(
        &reading_file,
        "{\"kwh_delivered\":14000,\"soil_health_score\":94,\"payout_love\":5000}\n\
         {\"kwh_delivered\":14150,\"soil_health_score\":94,\"payout_love\":5000}\n",
    )
    .unwrap();

    let endpoint = format!("{}/api/telemetry", srv.base_url());
    let daemon = Command::new(hw_bin)
        .args([
            "daemon",
            "--keyfile",
            keyfile.to_str().unwrap(),
            "--endpoint",
            &endpoint,
            "--reading-file",
            reading_file.to_str().unwrap(),
        ])
        .output()
        .expect("run daemon");
    assert!(
        daemon.status.success(),
        "daemon failed: stdout={} stderr={}",
        String::from_utf8_lossy(&daemon.stdout),
        String::from_utf8_lossy(&daemon.stderr)
    );

    // Verify server applied both daemon packets (leaves #2 and #3) and generated 2 STARK proofs!
    let mut state_resp = ureq::get(&format!("{}/api/state", srv.base_url()))
        .call()
        .unwrap();
    let state_val: Value =
        serde_json::from_str(&state_resp.body_mut().read_to_string().unwrap()).unwrap();
    assert_eq!(state_val["ledger_summary"]["balance_love"], 90_000);
    assert_eq!(state_val["ledger_summary"]["total_kwh_verified"], 28_150);
    assert_eq!(state_val["ledger_summary"]["stark_proofs_verified"], 2);
    assert_eq!(state_val["keys"]["hardware_remaining_leaves"], 60);

    srv.stop();
    let _ = std::fs::remove_dir_all(&tmp_dir);
}

#[test]
fn http_api_admin_token_protects_operator_writes_while_keeping_reads_public() {
    use newsatoshi_commons::server::{spawn_server, ServerConfig};
    use serde_json::{json, Value};

    let tmp_dir = std::env::temp_dir().join(format!(
        "newsatoshi_commons_auth_test_{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&tmp_dir);
    let state_file = tmp_dir.join("commons-ledger.json");

    let srv = spawn_server(ServerConfig {
        host: "127.0.0.1".to_string(),
        port: 0,
        state_file,
        seed_url: "http://127.0.0.1:1/status".to_string(),
        mainnet_seed_url: None,
        static_dir: None,
        admin_token: Some("super-secret-operator-token".to_string()),
        verify_only: false,
        external_public_keys: None,
    })
    .unwrap();
    let base = srv.base_url();

    // Public reads remain open and report auth_required: true
    let mut h_resp = ureq::get(&format!("{base}/api/health")).call().unwrap();
    let health: Value =
        serde_json::from_str(&h_resp.body_mut().read_to_string().unwrap()).unwrap();
    assert_eq!(health["ok"], true);
    assert_eq!(health["auth_required"], true);

    let mut s_resp = ureq::get(&format!("{base}/api/state")).call().unwrap();
    let st: Value = serde_json::from_str(&s_resp.body_mut().read_to_string().unwrap()).unwrap();
    assert_eq!(st["ledger_summary"]["balance_love"], 0);

    // Unauthenticated operator write MUST fail with 401
    let unauth = ureq::post(&format!("{base}/api/actions/reset"))
        .header("Content-Type", "application/json")
        .send("{}");
    assert!(unauth.is_err(), "unauthenticated reset must be rejected");

    // Authenticated operator write MUST succeed
    let mut authed = ureq::post(&format!("{base}/api/actions/record-funding"))
        .header("Content-Type", "application/json")
        .header("Authorization", "Bearer super-secret-operator-token")
        .send(
            json!({
                "l1_network": "testnet",
                "l1_txid": hex::encode(sha256(b"authed_l1_funding")),
                "amount_love": 1_000,
                "note": "auth test"
            })
            .to_string()
            .as_str(),
        )
        .unwrap();
    let authed_val: Value =
        serde_json::from_str(&authed.body_mut().read_to_string().unwrap()).unwrap();
    assert_eq!(authed_val["ok"], true);
    assert_eq!(authed_val["state"]["ledger_summary"]["balance_love"], 1_000);

    srv.stop();
    let _ = std::fs::remove_dir_all(&tmp_dir);
}

#[test]
fn verify_only_mode_holds_zero_private_keys_rejects_server_signing_and_accepts_offline_deeds_and_telemetry() {
    use newsatoshi_commons::server::{spawn_server, ServerConfig};
    use newsatoshi_commons::{
        ExternalDeedPacket, ExternalPublicKeysConfig, ExternalTelemetryPacket, SlhDsaKeypair,
    };
    use serde_json::{json, Value};

    // Generate completely isolated offline keypairs (NEVER passed into the server!)
    let offline_deed_xmss = MerkleLamportKeypair::new_64(sha256(b"OFFLINE_TRUSTEE_HSM_XMSS"));
    let offline_hw_xmss = MerkleLamportKeypair::new_64(sha256(b"OFFLINE_SOLAR_GATEWAY_XMSS"));
    let offline_deed_slh = SlhDsaKeypair::from_seed(sha256(b"OFFLINE_TRUSTEE_HSM_SLH"));
    let offline_hw_slh = SlhDsaKeypair::from_seed(sha256(b"OFFLINE_SOLAR_GATEWAY_SLH"));
    let rogue_slh = SlhDsaKeypair::from_seed(sha256(b"ROGUE_ATTACKER_SLH"));

    let ext_keys = ExternalPublicKeysConfig {
        deed_oracle_merkle_root: hex::encode(offline_deed_xmss.root()),
        hardware_merkle_root: hex::encode(offline_hw_xmss.root()),
        deed_oracle_slh_dsa_pk_hex: Some(offline_deed_slh.pk.to_hex()),
        hardware_slh_dsa_pk_hex: Some(offline_hw_slh.pk.to_hex()),
        max_bounty_payout: Some(25_000),
        reinvest_bps: Some(1_500),
        min_soil_health_score: Some(80),
    };

    let tmp_dir = std::env::temp_dir().join(format!(
        "newsatoshi_commons_verify_only_{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&tmp_dir);
    let state_file = tmp_dir.join("commons-ledger.json");

    let srv = spawn_server(ServerConfig {
        host: "127.0.0.1".to_string(),
        port: 0,
        state_file: state_file.clone(),
        seed_url: "http://127.0.0.1:1/status".to_string(),
        mainnet_seed_url: None,
        static_dir: None,
        admin_token: None,
        verify_only: true,
        external_public_keys: Some(ext_keys),
    })
    .unwrap();
    let base = srv.base_url();

    // 1. Health & state explicitly report verify_only: true and signing_keys_loaded: false
    let mut h_resp = ureq::get(&format!("{base}/api/health")).call().unwrap();
    let health: Value =
        serde_json::from_str(&h_resp.body_mut().read_to_string().unwrap()).unwrap();
    assert_eq!(health["verify_only"], true);
    assert_eq!(health["signing_keys_loaded"], false);

    // 2. Verify on-disk state file contains ZERO private seeds
    let disk_json = std::fs::read_to_string(&state_file).unwrap();
    assert!(
        !disk_json.contains("deed_oracle_xmss_seed_hex"),
        "verify-only disk ledger MUST NOT contain any private seed fields"
    );
    assert!(
        !disk_json.contains("hardware_slh_seed_hex"),
        "verify-only disk ledger MUST NOT contain any private SLH-DSA seeds"
    );

    // 3. Fund treasury via record-funding
    let mut f_resp = ureq::post(&format!("{base}/api/actions/record-funding"))
        .header("Content-Type", "application/json")
        .send(
            json!({
                "l1_network": "mainnet",
                "l1_txid": hex::encode(sha256(b"mainnet_real_funding_lock")),
                "amount_love": 100_000,
                "note": "Parcel #1 Trust Treasury"
            })
            .to_string()
            .as_str(),
        )
        .unwrap();
    let f_val: Value = serde_json::from_str(&f_resp.body_mut().read_to_string().unwrap()).unwrap();
    assert_eq!(f_val["ok"], true);

    // 4. Server-side signing endpoints MUST return 403 in verify-only mode
    let server_deed = ureq::post(&format!("{base}/api/actions/acquire-parcel"))
        .header("Content-Type", "application/json")
        .send(
            json!({
                "parcel_id": 1,
                "acres": 10.0,
                "price_love": 40_000,
                "deed_label": "Blocked Server Sign"
            })
            .to_string()
            .as_str(),
        );
    assert!(server_deed.is_err(), "server-side acquire-parcel must fail with 403");

    let server_tel = ureq::post(&format!("{base}/api/actions/telemetry"))
        .header("Content-Type", "application/json")
        .send(
            json!({
                "parcel_id": 1,
                "device_id": "unsigned-attempt",
                "kwh_delivered": 5_000,
                "soil_health_score": 92,
                "payout_love": 5_000
            })
            .to_string()
            .as_str(),
        );
    assert!(server_tel.is_err(), "unsigned telemetry action must fail with 403");

    // 5. Offline-signed ExternalDeedPacket from rogue key MUST be rejected
    let deed_hash = sha256(b"REAL_COUNTY_DEED_PARCEL_1");
    let rogue_deed = ExternalDeedPacket::sign_slh_dsa(
        &rogue_slh,
        1,
        10.0,
        40_000,
        "Rogue Forgery",
        deed_hash,
        0,
    );
    let mut r_rogue = ureq::post(&format!("{base}/api/deeds"))
        .header("Content-Type", "application/json")
        .send(serde_json::to_string(&rogue_deed).unwrap().as_str())
        .unwrap();
    let rogue_val: Value =
        serde_json::from_str(&r_rogue.body_mut().read_to_string().unwrap()).unwrap();
    assert_eq!(rogue_val["ok"], false, "unauthorized deed signer must be rejected");

    // 6. Legitimate offline-signed ExternalDeedPacket succeeds & proves STARK
    let valid_deed = ExternalDeedPacket::sign_slh_dsa(
        &offline_deed_slh,
        1,
        10.0,
        40_000,
        "Parcel #1 Perpetual Purpose Trust Deed",
        deed_hash,
        0,
    );
    let mut r_deed = ureq::post(&format!("{base}/api/deeds"))
        .header("Content-Type", "application/json")
        .send(serde_json::to_string(&valid_deed).unwrap().as_str())
        .unwrap();
    let deed_val: Value =
        serde_json::from_str(&r_deed.body_mut().read_to_string().unwrap()).unwrap();
    assert_eq!(deed_val["ok"], true);
    assert_eq!(deed_val["state"]["ledger_summary"]["balance_love"], 60_000);
    assert_eq!(deed_val["state"]["ledger_summary"]["parcels_acquired_count"], 1);
    assert_eq!(deed_val["stark_proof"]["verified"], true);

    // 7. Legitimate offline-signed ExternalTelemetryPacket succeeds in verify-only mode
    let hw_pkt = ExternalTelemetryPacket::sign_slh_dsa(
        &offline_hw_slh,
        "field-solar-gateway-01",
        1,
        9_500,
        93,
        5_000,
        0,
        1_795_380_000,
    );
    let mut r_hw = ureq::post(&format!("{base}/api/telemetry"))
        .header("Content-Type", "application/json")
        .send(serde_json::to_string(&hw_pkt).unwrap().as_str())
        .unwrap();
    let hw_val: Value = serde_json::from_str(&r_hw.body_mut().read_to_string().unwrap()).unwrap();
    assert_eq!(hw_val["ok"], true);
    assert_eq!(hw_val["state"]["ledger_summary"]["balance_love"], 55_000);
    assert_eq!(hw_val["state"]["ledger_summary"]["total_kwh_verified"], 9_500);
    assert_eq!(hw_val["stark_proof"]["verified"], true);

    srv.stop();
    let _ = std::fs::remove_dir_all(&tmp_dir);
}

#[test]
fn live_l1_anchor_verification_and_dual_chain_seed_polling() {
    use newsatoshi_commons::server::{spawn_server, ServerConfig};
    use serde_json::{json, Value};

    // Mock L1 node serving /status (testnet), /mainnet/status (mainnet), and /api/lock?hash=...
    let mock_l1 = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let l1_addr = mock_l1.server_addr().to_ip().unwrap().to_string();
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop_clone = std::sync::Arc::clone(&stop);
    let l1_thread = thread::spawn(move || {
        let testnet_status = r#"{"network":"testnet","proofs":"test","fee":300000,"version":"0.1.0","genesis":"00000fd814d823a0bc64a84135868402998cff81e28a85b2bede5f1125d0a394","height":22137,"tip":"0000007e7c70f28b037204ae836e9818c8e785a4eda3e3641b1c00267b1da201","tip_time":1795380010,"now":1795380090,"bits":"1e00aec5","difficulty":0.0057,"chainwork":"0000","peers":4,"known_peers":1,"best_header":22137,"mempool":0,"issued":110685000000000,"notes":22156,"nullifiers":14,"locks":1,"conditions":0,"auxpow_chainid":20051,"recent":[{"height":22137,"hash":"0000007e7c70f28b037204ae836e9818c8e785a4eda3e3641b1c00267b1da201","time":1795380010,"txs":1,"bytes":207,"merge_mined":false}]}"#;
        let mainnet_status = r#"{"network":"mainnet","proofs":"main","fee":600000,"version":"0.1.0","genesis":"000000006ba30cc3c86ecaabcb962287e9d5fd0877c27f4b33661b4d1d3033f7","height":0,"tip":"000000006ba30cc3c86ecaabcb962287e9d5fd0877c27f4b33661b4d1d3033f7","tip_time":1794325578,"now":1795380119,"bits":"1d00ffff","difficulty":1,"chainwork":"0000000000000000000000000000000000000000000000000000000100010001","peers":4,"known_peers":1,"best_header":0,"mempool":0,"issued":0,"notes":0,"nullifiers":0,"locks":0,"conditions":0,"auxpow_chainid":20051,"recent":[{"height":0,"hash":"000000006ba30cc3c86ecaabcb962287e9d5fd0877c27f4b33661b4d1d3033f7","time":1794325578,"txs":0,"bytes":0,"merge_mined":false}]}"#;

        while !stop_clone.load(std::sync::atomic::Ordering::SeqCst) {
            if let Ok(Some(req)) = mock_l1.recv_timeout(Duration::from_millis(50)) {
                let url = req.url().to_string();
                let body = if url.starts_with("/mainnet/status") {
                    mainnet_status.to_string()
                } else if let Some(hash) = url.strip_prefix("/api/lock?hash=") {
                    json!({
                        "locks": [{
                            "found": true,
                            "id": "d42929062155a119894219b36e312eb58dc14c0c1e763c2fb0072b1b6edd1c45",
                            "state": "open",
                            "confirmed": true,
                            "amount": 1000000,
                            "hash": hash,
                            "timeout": 23117,
                            "claim": "ns1bc03485c35c661a5f6685a52f19351a143b44994df2943c20285b2e9876f68fa10452da1",
                            "refund": "ns1bc03485c35c661a5f6685a52f19351a143b44994df2943c20285b2e9876f68fa10452da1",
                            "at": 22118,
                            "confirmations": 20,
                            "height": 22137
                        }]
                    })
                    .to_string()
                } else {
                    testnet_status.to_string()
                };
                let _ = req.respond(tiny_http::Response::from_string(body));
            }
        }
    });

    let tmp_dir = std::env::temp_dir().join(format!(
        "newsatoshi_commons_dual_seed_{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&tmp_dir);
    let state_file = tmp_dir.join("commons-ledger.json");

    let srv = spawn_server(ServerConfig {
        host: "127.0.0.1".to_string(),
        port: 0,
        state_file,
        seed_url: format!("http://{l1_addr}/status"),
        mainnet_seed_url: Some(format!("http://{l1_addr}/mainnet/status")),
        static_dir: None,
        admin_token: Some("op-tok".to_string()),
        verify_only: true,
        external_public_keys: None,
    })
    .unwrap();
    let base = srv.base_url();

    // 1. Dual-chain /api/seed reports BOTH live testnet and live mainnet!
    let mut s_resp = ureq::get(&format!("{base}/api/seed")).call().unwrap();
    let seed: Value = serde_json::from_str(&s_resp.body_mut().read_to_string().unwrap()).unwrap();
    assert_eq!(seed["live"], true);
    assert_eq!(seed["status"]["height"], 22137);
    assert_eq!(seed["mainnet_live"], true);
    assert_eq!(seed["mainnet_status"]["network"], "mainnet");
    assert_eq!(seed["mainnet_status"]["proofs"], "main");
    assert_eq!(seed["mainnet_status"]["height"], 0);
    assert_eq!(seed["mainnet_status"]["difficulty"], 1.0);

    // 2. Prepare anchor (authed), then publicly verify on-chain WITHOUT admin token!
    let mut a_resp = ureq::post(&format!("{base}/api/actions/anchor"))
        .header("Content-Type", "application/json")
        .header("Authorization", "Bearer op-tok")
        .send(r#"{"network":"testnet"}"#)
        .unwrap();
    let anc: Value = serde_json::from_str(&a_resp.body_mut().read_to_string().unwrap()).unwrap();
    assert_eq!(anc["ok"], true);
    assert_eq!(anc["receipt"]["verified_on_l1"], false);
    let anchor_hash = anc["receipt"]["anchor_hash"].as_str().unwrap().to_string();

    let mut v_resp = ureq::post(&format!("{base}/api/actions/verify-anchor"))
        .header("Content-Type", "application/json")
        .send(json!({ "anchor_hash": anchor_hash }).to_string().as_str())
        .unwrap();
    let ver: Value = serde_json::from_str(&v_resp.body_mut().read_to_string().unwrap()).unwrap();
    assert_eq!(ver["ok"], true);
    assert_eq!(ver["verified_on_l1"], true);
    assert_eq!(ver["receipt"]["verified_on_l1"], true);
    assert_eq!(ver["receipt"]["broadcast_status"], "broadcast");
    assert_eq!(ver["receipt"]["l1_confirmed_height"], 22118);
    assert_eq!(ver["receipt"]["l1_confirmations"], 20);
    assert_eq!(
        ver["receipt"]["l1_lock_id"],
        "d42929062155a119894219b36e312eb58dc14c0c1e763c2fb0072b1b6edd1c45"
    );

    srv.stop();
    stop.store(true, std::sync::atomic::Ordering::SeqCst);
    let _ = l1_thread.join();
    let _ = std::fs::remove_dir_all(&tmp_dir);
}
