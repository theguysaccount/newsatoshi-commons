#!/usr/bin/env python3
"""Unit tests for the zero-dependency Newsatoshi HTTP 402 PayWord Python SDK (v0.15.0)."""

import os
import tempfile
import unittest

import newsatoshi_402 as ns402


class Newsatoshi402PythonSdkTests(unittest.TestCase):
    def test_chain_verify_incremental_forgery_and_anti_replay_gate(self) -> None:
        self.assertEqual(ns402.MAX_PAYWORD_TICKS, 1_000_000)
        wallet = ns402.generate_payword_chain(15, 2_000)
        self.assertEqual(len(wallet.tip_hex), 64)

        p1 = wallet.next_payment(1)
        res1 = ns402.verify_payword_token(p1["tip_hex"], p1["token_hex"], p1["ticks"], 2_000)
        self.assertTrue(res1["valid"])
        self.assertEqual(res1["hashes_evaluated"], 1)
        self.assertEqual(res1["total_paid_base_units"], 2_000)

        bad = ns402.verify_payword_token(p1["tip_hex"], "00" * 32, 1, 2_000)
        self.assertFalse(bad["valid"])

        gate = ns402.PayWordGate(min_ticks_increment=1, unit_price_base_units=2_000, allow_unfunded_demo=True)
        g1 = gate.verify_header(p1["header_value"])
        self.assertTrue(g1["paid"])
        self.assertFalse(g1["collectible_on_chain"])
        self.assertEqual(g1["funding_status"], "unfunded_demo")
        self.assertEqual(g1["delta_paid_base_units"], 2_000)

        # Exact replay MUST fail
        replay = gate.verify_header(p1["header_value"])
        self.assertFalse(replay["paid"])

        # Advancing by 2 ticks verifies incrementally (only 2 hashes) and bills delta (4_000)
        p3 = wallet.next_payment(2)
        g3 = gate.verify_header(p3["header_value"])
        self.assertTrue(g3["paid"])
        self.assertEqual(g3["verification_mode"], "incremental")
        self.assertEqual(g3["hashes_evaluated"], 2)
        self.assertEqual(g3["ticks"], 3)
        self.assertEqual(g3["delta_paid_base_units"], 4_000)

    def test_funding_requirement_and_persistent_state_across_restarts(self) -> None:
        wallet = ns402.generate_payword_chain(10_005, 1_000)
        p10k = wallet.next_payment(10_000)

        # Unfunded verifier rejects cleanly
        reject_gate = ns402.PayWordGate(
            require_funding=True,
            funding_verifier=lambda _req: {"funded": False, "reason": "no open lock"},
        )
        rej = reject_gate.verify_header(p10k["header_value"])
        self.assertFalse(rej["paid"])
        self.assertEqual(rej["funding_status"], "rejected")

        # Funded verifier + persistent state file survives restart and runs O(1) incremental on tick #10,001
        with tempfile.TemporaryDirectory() as td:
            sf = os.path.join(td, "watermarks.json")
            funded_fn = lambda _req: {"funded": True, "amount_base_units": 50_000_000, "lock_id": "abc"}
            g1 = ns402.PayWordGate(require_funding=True, funding_verifier=funded_fn, state_file=sf)
            r1 = g1.verify_header(p10k["header_value"])
            self.assertTrue(r1["paid"])
            self.assertTrue(r1["collectible_on_chain"])
            self.assertEqual(r1["funding_status"], "verified_onchain_lock")

            # Reload fresh gate from disk
            g2 = ns402.PayWordGate(require_funding=True, funding_verifier=funded_fn, state_file=sf)
            self.assertFalse(g2.verify_header(p10k["header_value"])["paid"])
            p10001 = wallet.next_payment(1)
            r2 = g2.verify_header(p10001["header_value"])
            self.assertTrue(r2["paid"])
            self.assertEqual(r2["verification_mode"], "incremental")
            self.assertEqual(r2["hashes_evaluated"], 1)
            self.assertTrue(r2["collectible_on_chain"])

    def test_mcp_client_rejects_non_http_schemes(self) -> None:
        with self.assertRaises(ValueError):
            ns402.NewsatoshiMcpClient("file:///etc/passwd")

    def test_fetch_with_newsatoshi_402_auto_retries_with_payword(self) -> None:
        import http.server
        import json
        import threading

        gate = ns402.PayWordGate(min_ticks_increment=1, unit_price_base_units=1_000, allow_unfunded_demo=True)

        class Handler(http.server.BaseHTTPRequestHandler):
            def do_POST(self) -> None:
                length = int(self.headers.get("Content-Length", "0"))
                _ = self.rfile.read(length)
                hdr = self.headers.get("X-Newsatoshi-PayWord")
                res = gate.verify_header(hdr)
                if not res["paid"]:
                    self.send_response(402)
                    self.send_header("Content-Type", "application/json")
                    self.end_headers()
                    self.wfile.write(json.dumps({"error": "payment required"}).encode("utf-8"))
                    return
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.end_headers()
                self.wfile.write(json.dumps({"ok": True, "ticks": res["ticks"]}).encode("utf-8"))

            def log_message(self, format: str, *args: object) -> None:
                return

        srv = http.server.HTTPServer(("127.0.0.1", 0), Handler)
        port = srv.server_address[1]
        t = threading.Thread(target=srv.serve_forever, daemon=True)
        t.start()
        try:
            wallet = ns402.generate_payword_chain(10, 1_000)
            status, body, _ = ns402.fetch_with_newsatoshi_402(
                f"http://127.0.0.1:{port}/scan",
                wallet,
                method="POST",
                body=b'{"chain":"base"}',
                headers={"Content-Type": "application/json"},
            )
            self.assertEqual(status, 200)
            parsed = json.loads(body.decode("utf-8"))
            self.assertTrue(parsed["ok"])
            self.assertEqual(parsed["ticks"], 1)
        finally:
            srv.shutdown()
            srv.server_close()

    def test_rfc0022_channel_merkle_proof_and_70_byte_claim_encoding(self) -> None:
        import hashlib
        import http.server
        import json
        import struct
        import threading

        wallet = ns402.generate_payword_chain(1_000, 10_000)
        seller = "ns1seller00000000000000000000000000000000000000000000000000000000"
        cid = "ab" * 32
        vhash = "cd" * 32
        root = hashlib.sha256(b"nsMKL" + bytes.fromhex(cid) + bytes.fromhex(vhash)).hexdigest()
        self.assertTrue(
            ns402.verify_channel_merkle_proof({"root": root, "key": cid, "value_hash": vhash, "siblings": []})
        )
        self.assertFalse(
            ns402.verify_channel_merkle_proof({"root": root, "key": cid, "value_hash": "00" * 32, "siblings": []})
        )

        p237 = wallet.next_payment(237)
        fake_sig_hex = "11" * 7856
        claim_tx = ns402.encode_channel_claim_tx(cid, 237, p237["token_hex"], fake_sig_hex)
        self.assertEqual(len(claim_tx), 7930)
        self.assertEqual(claim_tx[0], 2)
        self.assertEqual(claim_tx[1], 11)
        self.assertEqual(struct.unpack("<I", claim_tx[34:38])[0], 237)
        self.assertEqual(struct.unpack("<I", claim_tx[70:74])[0], 7856)
        self.assertEqual(len(ns402.encode_channel_expire_tx(cid)), 34)

        received_tx = {}

        class MockNode(http.server.BaseHTTPRequestHandler):
            def do_GET(self) -> None:
                if self.path.startswith("/api/channel?tip="):
                    payload = {
                        "height": 5000,
                        "channels": [
                            {
                                "found": True,
                                "id": cid,
                                "state": "open",
                                "confirmed": True,
                                "tip": wallet.tip_hex,
                                "max_ticks": 1_000,
                                "unit_price": 10_000,
                                "claim_fee": 5_000,
                                "budget": 10_000_000,
                                "total_deposit": 10_005_000,
                                "timeout": 5100,
                                "seller": seller,
                                "refund": "ns1buyer000000000000000000000000000000000000000000000000000000000",
                                "confirmations": 2,
                                "height": 5000,
                                "merkle_proof": {"root": root, "key": cid, "value_hash": vhash, "siblings": []},
                            }
                        ],
                    }
                    self.send_response(200)
                    self.send_header("Content-Type", "application/json")
                    self.end_headers()
                    self.wfile.write(json.dumps(payload).encode("utf-8"))
                    return
                self.send_response(404)
                self.end_headers()

            def do_POST(self) -> None:
                if self.path == "/api/tx":
                    n = int(self.headers.get("Content-Length", "0"))
                    received_tx["bytes"] = self.rfile.read(n)
                    self.send_response(200)
                    self.send_header("Content-Type", "application/json")
                    self.end_headers()
                    self.wfile.write(json.dumps({"ok": True, "txid": "ef" * 32}).encode("utf-8"))
                    return
                self.send_response(404)
                self.end_headers()

            def log_message(self, format: str, *args: object) -> None:
                return

        srv = http.server.HTTPServer(("127.0.0.1", 0), MockNode)
        port = srv.server_address[1]
        t = threading.Thread(target=srv.serve_forever, daemon=True)
        t.start()
        try:
            base = f"http://127.0.0.1:{port}"
            gate = ns402.PayWordGate(
                unit_price_base_units=10_000,
                merchant_address=seller,
                require_funding=True,
                funding_rail="channel",
                seed_base_url=base,
            )
            res = gate.verify_header(p237["header_value"])
            self.assertTrue(res["paid"])
            self.assertTrue(res["collectible_on_chain"])
            self.assertTrue(res["unilateral_claim_ready"])
            self.assertEqual(res["funding_status"], "verified_onchain_channel")
            self.assertEqual(res["channel_id"], cid)

            sub = ns402.submit_l1_channel_claim(
                cid, 237, p237["token_hex"], fake_sig_hex, seed_base_url=base
            )
            self.assertTrue(sub["ok"])
            self.assertEqual(sub["tx_bytes"], 7930)
            self.assertEqual(received_tx["bytes"], claim_tx)
        finally:
            srv.shutdown()
            srv.server_close()

    def test_clearinghouse_scoped_session_zktls_and_rollup_withdraw(self):
        # 1. RFC-0025 Scoped Agent Session Policy enforcement
        sess = ns402.ScopedAgentSession(
            session_id="agent-ops-1",
            max_per_call=10_000,
            max_per_hour=25_000,
            max_total=50_000,
            min_counterparty_bond=100_000_000,
            max_counterparty_slashes=0,
            allowed_endpoints=["https://api.compute.example/"],
            profit_sweep_address="ns1treasury",
            expiry_unix=2_000_000_000,
        )
        # Missing bond -> rejected
        r_nobond = sess.authorize_spend(5_000, "https://api.compute.example/v1", None, now_unix=1_800_000_000)
        self.assertFalse(r_nobond["allowed"])
        # Slashed counterparty -> rejected
        r_slash = sess.authorize_spend(
            5_000,
            "https://api.compute.example/v1",
            {"found": True, "stake": 200_000_000, "slash_count": 1},
            now_unix=1_800_000_000,
        )
        self.assertFalse(r_slash["allowed"])
        # Clean bonded counterparty -> allowed!
        good_bond = {"found": True, "stake": 500_000_000, "slash_count": 0}
        r_ok = sess.authorize_spend(10_000, "https://api.compute.example/v1", good_bond, now_unix=1_800_000_000)
        self.assertTrue(r_ok["allowed"])
        self.assertEqual(r_ok["total_spent"], 10_000)
        # Disallowed endpoint -> rejected
        r_bad_ep = sess.authorize_spend(5_000, "https://evil.example/drain", good_bond, now_unix=1_800_000_000)
        self.assertFalse(r_bad_ep["allowed"])

        # 2. Tier-1 zkTLS Web2 Attestation Verifier
        att = {
            "domain": "api.wise.com",
            "http_status": 200,
            "timestamp_unix": 1_800_000_000,
            "proof_digest_hex": "aa" * 32,
            "response_body": {"status": "COMPLETED", "amount_usd": 100, "recipient": "acct-99"},
        }
        zk_ok = ns402.verify_zktls_attestation(
            att,
            expected_domain="api.wise.com",
            required_fields={"status": "COMPLETED", "amount_usd": lambda v: v >= 100},
            now_unix=1_800_000_050,
        )
        self.assertTrue(zk_ok["valid"])
        self.assertEqual(len(zk_ok["receipt_hash_hex"]), 64)

        # 3. RFC-0023 RollupWithdraw binary tx encoder + bondCommitMessageHex
        wtx = ns402.encode_rollup_withdraw_tx(
            "11" * 32,
            1,
            "22" * 32,
            "33" * 32,
            400_000_000,
            10_000,
            {"key": "22" * 32, "value_hash": "44" * 32, "siblings": ["55" * 32]},
        )
        self.assertEqual(wtx[0], 2)
        self.assertEqual(wtx[1], 19)
        self.assertEqual(len(wtx), 2 + 32 + 8 + 32 + 32 + 8 + 8 + 32 + 32 + 2 + 32)
        self.assertEqual(len(ns402.bond_commit_message_hex("11" * 32, 7, "aa" * 32)), 64)

    def test_rfc0027_mission_launchpad_curve_and_digests(self):
        buy = ns402.quote_mission_curve_buy(0, 0, 500, 2_000_000_000)
        self.assertEqual(buy["fee_to_bounty_pool"], 100_000_000)
        self.assertEqual(buy["net_to_curve"], 1_900_000_000)
        self.assertEqual(buy["founder_cut"], 0)
        self.assertEqual(buy["shares_minted"], 1_378_404)
        self.assertEqual(buy["fee_to_bounty_pool"] + buy["net_to_curve"], buy["amount"])

        redeem = ns402.quote_mission_curve_redeem(
            buy["new_curve_reserve"],
            buy["new_total_shares"],
            500,
            buy["shares_minted"] // 2,
            5_000,
        )
        self.assertEqual(redeem["founder_cut"], 0)
        self.assertEqual(
            redeem["fee_to_bounty_pool"] + redeem["user_payout"] + redeem["miner_fee"],
            redeem["gross_exit"],
        )
        self.assertGreater(redeem["fee_to_bounty_pool"], 0)

        post_hex = ns402.mission_post_task_message_hex("11" * 32, 1, "22" * 32, 200_010_000)
        claim_hex = ns402.mission_claim_task_message_hex("11" * 32, 1, "22" * 32, "33" * 32, "44" * 32, 10_000)
        self.assertEqual(len(post_hex), 64)
        self.assertEqual(len(claim_hex), 64)
        self.assertNotEqual(post_hex, claim_hex)

    def test_v030_base_usdc_payword_channel_gate_and_calldata(self):
        wallet = ns402.generate_payword_chain(100, 1_000)
        recipient = "0x1744535521EB420be9700475400455a4Ad0EBEa2"
        payer = "0x1111111111111111111111111111111111111111"
        channel_id = "0x" + "ab" * 32
        expiry = int(ns402.time.time()) + 86_400

        open_cd = ns402.encode_base_open_channel_calldata(
            recipient=recipient,
            root_hex=wallet.tip_hex,
            unit_amount=1_000,
            max_steps=100,
            expiry_unix=expiry,
        )
        self.assertTrue(open_cd.startswith("0xfbbd272c"))
        self.assertEqual(len(open_cd), 10 + 6 * 64)

        inject_cd = ns402.encode_fair_curve_inject_revenue_calldata(5_000)
        self.assertEqual(inject_cd, "0xe224ef38" + f"{5000:064x}")

        fake_words = (
            "0x"
            + payer[2:].rjust(64, "0")
            + recipient[2:].lower().rjust(64, "0")
            + ns402.BASE_USDC_ADDRESS[2:].lower().rjust(64, "0")
            + wallet.tip_hex
            + wallet.tip_hex
            + f"{1000:064x}"
            + f"{100:064x}"
            + f"{0:064x}"
            + f"{expiry:064x}"
            + f"{0:064x}"
        )
        decoded = ns402.decode_base_payword_channel_words(fake_words, channel_id)
        self.assertTrue(decoded["found"])
        self.assertEqual(decoded["remaining_amount"], 100_000)

        gate = ns402.BasePayWordGate(
            recipient=recipient,
            unit_amount=1_000,
            channel_fetcher=lambda _cid: decoded,
        )
        self.assertFalse(gate.verify_headers({})["ok"])

        last = None
        for i in range(1, 26):
            last = wallet.next_payment(1)
            res = gate.verify_headers(
                {
                    "X-PayWord-Channel": channel_id,
                    "X-PayWord-Index": str(last["ticks"]),
                    "X-PayWord-Preimage": "0x" + last["token_hex"],
                }
            )
            self.assertTrue(res["ok"])
            self.assertEqual(res["delta_steps"], 1)
            self.assertEqual(res["unclaimed_steps"], i)

        # Replay rejected
        replay = gate.verify_headers(
            {
                "X-PayWord-Channel": channel_id,
                "X-PayWord-Index": str(last["ticks"]),
                "X-PayWord-Preimage": "0x" + last["token_hex"],
            }
        )
        self.assertFalse(replay["ok"])

        batch = gate.export_settlement_batch(channel_id)
        self.assertEqual(batch["steps_to_claim"], 25)
        self.assertEqual(batch["payout_raw"], 25_000)
        self.assertTrue(batch["calldata"].startswith("0xe0ab9c6b"))

    def test_multichain_registry_and_autopay_session_402_to_200(self) -> None:
        self.assertEqual(ns402.EVM_PAYWORD_CHAINS["base"]["chain_id"], 8453)
        self.assertEqual(ns402.EVM_PAYWORD_CHAINS["optimism"]["chain_id"], 10)
        self.assertEqual(ns402.EVM_PAYWORD_CHAINS["arbitrum"]["chain_id"], 42161)
        self.assertEqual(
            ns402.EVM_PAYWORD_CHAINS["optimism"]["payword_channel"],
            ns402.EVM_PAYWORD_CHAINS["arbitrum"]["payword_channel"],
        )

        wallet = ns402.generate_payword_chain(10, 1_000)
        calls = []

        def fake_opener(req, _timeout):
            calls.append(req.get_method())
            sig = req.get_header("Payment-signature")
            if not sig:
                return 402, '{"error":"payment_required"}'
            import base64, json
            p = json.loads(base64.b64decode(sig).decode("utf-8"))
            v = ns402.verify_payword_token(p["rootHex"], p["tokenHex"], p["index"], 1_000)
            self.assertTrue(v["valid"])
            return 200, json.dumps({"ok": True, "network": p["network"], "step": p["index"]})

        sess = ns402.AutoPaySession(wallet=wallet, chain="arbitrum", opener=fake_opener)
        status, body = sess.request("https://commons.newsatoshi.org/api/v2/paywords/try-402", method="POST")
        self.assertEqual(status, 200)
        self.assertIn("eip155:42161", body)
        self.assertEqual(len(calls), 2)


if __name__ == "__main__":
    unittest.main()
