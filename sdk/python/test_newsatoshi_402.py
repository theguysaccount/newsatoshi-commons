#!/usr/bin/env python3
"""Unit tests for the zero-dependency Newsatoshi HTTP 402 PayWord Python SDK."""

import unittest

import newsatoshi_402 as ns402


class Newsatoshi402PythonSdkTests(unittest.TestCase):
    def test_chain_verify_forgery_and_anti_replay_gate(self) -> None:
        wallet = ns402.generate_payword_chain(15, 2_000)
        self.assertEqual(len(wallet.tip_hex), 64)

        p1 = wallet.next_payment(1)
        res1 = ns402.verify_payword_token(p1["tip_hex"], p1["token_hex"], p1["ticks"], 2_000)
        self.assertTrue(res1["valid"])
        self.assertEqual(res1["total_paid_base_units"], 2_000)

        bad = ns402.verify_payword_token(p1["tip_hex"], "00" * 32, 1, 2_000)
        self.assertFalse(bad["valid"])

        gate = ns402.PayWordGate(min_ticks_increment=1, unit_price_base_units=2_000)
        g1 = gate.verify_header(p1["header_value"])
        self.assertTrue(g1["paid"])
        self.assertEqual(g1["delta_paid_base_units"], 2_000)

        # Exact replay MUST fail
        replay = gate.verify_header(p1["header_value"])
        self.assertFalse(replay["paid"])

        # Advancing by 2 ticks succeeds and bills strictly delta (4_000)
        p3 = wallet.next_payment(2)
        g3 = gate.verify_header(p3["header_value"])
        self.assertTrue(g3["paid"])
        self.assertEqual(g3["ticks"], 3)
        self.assertEqual(g3["delta_paid_base_units"], 4_000)

    def test_mcp_client_rejects_non_http_schemes(self) -> None:
        with self.assertRaises(ValueError):
            ns402.NewsatoshiMcpClient("file:///etc/passwd")

    def test_fetch_with_newsatoshi_402_auto_retries_with_payword(self) -> None:
        import http.server
        import json
        import threading

        gate = ns402.PayWordGate(min_ticks_increment=1, unit_price_base_units=1_000)

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


if __name__ == "__main__":
    unittest.main()
