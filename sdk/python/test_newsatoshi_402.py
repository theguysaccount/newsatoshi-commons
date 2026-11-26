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
        claim_tx = ns402.encode_channel_claim_tx(cid, 237, p237["token_hex"])
        self.assertEqual(len(claim_tx), 70)
        self.assertEqual(claim_tx[0], 2)
        self.assertEqual(claim_tx[1], 11)
        self.assertEqual(struct.unpack("<I", claim_tx[34:38])[0], 237)
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

            sub = ns402.submit_l1_channel_claim(cid, 237, p237["token_hex"], seed_base_url=base)
            self.assertTrue(sub["ok"])
            self.assertEqual(sub["tx_bytes"], 70)
            self.assertEqual(received_tx["bytes"], claim_tx)
        finally:
            srv.shutdown()
            srv.server_close()


if __name__ == "__main__":
    unittest.main()
