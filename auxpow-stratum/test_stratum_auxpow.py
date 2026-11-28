#!/usr/bin/env python3
"""Unit + live TCP Stratum v1 protocol tests for Newsatoshi AuxPoW Stratum Server."""

import asyncio
import json
import struct
import unittest

import stratum_auxpow as sa


class FakeAuxRpc:
    """Deterministic fake `newsatoshi` RPC for offline CI unit tests."""

    def __init__(self, target_hex: str) -> None:
        self.target_hex = target_hex
        self.aux_hash = "aa" * 32
        self.height = 1
        self.submitted = []

    def call(self, method: str, params=None):
        params = params or []
        if method in ("getauxblock", "createauxblock"):
            return {
                "hash": self.aux_hash,
                "chainid": sa.AUXPOW_CHAIN_ID,
                "previousblockhash": "00" * 32,
                "coinbasevalue": 5_000_000_000,
                "bits": "207fffff",
                "height": self.height,
                "_target": self.target_hex,
            }
        if method == "submitauxblock":
            self.submitted.append((params[0], params[1]))
            self.height += 1
            self.aux_hash = "bb" * 32
            return True
        raise AssertionError(f"unexpected RPC method: {method}")


class AuxPowStratumTests(unittest.TestCase):
    def test_prevhash_stratum_roundtrip(self) -> None:
        display = "0000000000000000000123456789abcdef0123456789abcdef0123456789abcd"
        stratum_prev = sa.prevhash_to_stratum(display)
        internal = sa.stratum_prevhash_to_internal(stratum_prev)
        self.assertEqual(internal, bytes.fromhex(display)[::-1])

    def test_bip34_height_encoding(self) -> None:
        self.assertEqual(sa.encode_bip34_height(1), b"\x51")
        self.assertEqual(sa.encode_bip34_height(16), b"\x60")
        self.assertEqual(sa.encode_bip34_height(17), b"\x01\x11")
        self.assertEqual(sa.encode_bip34_height(128), b"\x02\x80\x00")
        self.assertEqual(sa.encode_bip34_height(850_000), b"\x03\x50\xf8\x0c")

    def test_coinbase_contains_valid_merged_mining_commitment(self) -> None:
        aux_hash = "12" * 32
        coinb1, coinb2 = sa.build_aux_coinbase_parts(aux_hash, bip34_height=3087)
        full_cb = coinb1 + b"\x00\x00\x00\x01" + (b"\x00" * (sa.EXTRANONCE2_SIZE - 1) + b"\x02") + coinb2
        magic_pos = full_cb.find(sa.MERGED_MINING_MAGIC)
        self.assertNotEqual(magic_pos, -1)
        root = full_cb[magic_pos + 4 : magic_pos + 36]
        size, nonce = struct.unpack("<II", full_cb[magic_pos + 36 : magic_pos + 44])
        self.assertEqual(root.hex(), aux_hash)
        self.assertEqual(size, 1)
        self.assertEqual(nonce, 0)

    def test_merkle_branch_fold_matches_full_tree(self) -> None:
        cb = sa.sha256d(b"coinbase")
        t1 = sa.sha256d(b"tx1")
        t2 = sa.sha256d(b"tx2")
        branch = sa.merkle_branch_for_coinbase([t1, t2])
        expected_root = sa.sha256d(sa.sha256d(cb + t1) + sa.sha256d(t2 + t2))
        self.assertEqual(sa.fold_merkle_root(cb, branch), expected_root)

    def test_live_tcp_stratum_handshake_and_auxpow_submit(self) -> None:
        async def run_flow() -> None:
            # Regtest-easy target: 0x7fffff00... (little-endian)
            easy_target_int = 0x7FFFFF << (8 * 29)
            target_le_hex = easy_target_int.to_bytes(32, "little").hex()
            fake_rpc = FakeAuxRpc(target_le_hex)
            srv = sa.AuxPowStratumServer(ns_rpc=fake_rpc, poll_interval=60.0)
            srv.refresh_template_sync()

            tcp_server = await asyncio.start_server(srv.handle_stratum_client, "127.0.0.1", 0)
            port = tcp_server.sockets[0].getsockname()[1]

            try:
                reader, writer = await asyncio.open_connection("127.0.0.1", port)

                async def send(obj):
                    writer.write((json.dumps(obj) + "\n").encode("utf-8"))
                    await writer.drain()

                async def recv():
                    line = await asyncio.wait_for(reader.readline(), timeout=5.0)
                    return json.loads(line.decode("utf-8"))

                # 1. BIP-320 version-rolling configure
                await send(
                    {
                        "id": 1,
                        "method": "mining.configure",
                        "params": [["version-rolling"], {"version-rolling.mask": "1fffe000"}],
                    }
                )
                cfg = await recv()
                self.assertTrue(cfg["result"]["version-rolling"])

                # 2. Subscribe -> clean subscribe result (no premature notify before authorize)
                await send({"id": 2, "method": "mining.subscribe", "params": ["bitaxe-test/1.0"]})
                sub = await recv()
                en1_hex = sub["result"][1]
                self.assertEqual(len(en1_hex), 8)
                self.assertEqual(sub["result"][2], sa.EXTRANONCE2_SIZE)

                # 3. Authorize -> receives authorize result, then set_difficulty and mining.notify
                await send({"id": 3, "method": "mining.authorize", "params": ["bitaxe.rig1", "x"]})
                auth = await recv()
                self.assertTrue(auth["result"])
                diff_msg = await recv()
                self.assertEqual(diff_msg["method"], "mining.set_difficulty")
                notify = await recv()
                self.assertEqual(notify["method"], "mining.notify")
                job_id, prevhash, coinb1_hex, coinb2_hex, branch_hex, ver_hex, nbits_hex, ntime_hex, _ = notify[
                    "params"
                ]

                # 4. Grind real SHA-256d share meeting regtest target
                en2_hex = "00" * (sa.EXTRANONCE2_SIZE - 1) + "01"
                coinbase = bytes.fromhex(coinb1_hex + en1_hex + en2_hex + coinb2_hex)
                merkle_root = sa.fold_merkle_root(
                    sa.sha256d(coinbase), [bytes.fromhex(h) for h in branch_hex]
                )
                prev_internal = sa.stratum_prevhash_to_internal(prevhash)
                ver_u32 = int(ver_hex, 16)
                nbits_u32 = int(nbits_hex, 16)
                ntime_u32 = int(ntime_hex, 16)

                found_nonce = None
                for nonce in range(100_000):
                    hdr = (
                        struct.pack("<I", ver_u32)
                        + prev_internal
                        + merkle_root
                        + struct.pack("<III", ntime_u32, nbits_u32, nonce)
                    )
                    if int.from_bytes(sa.sha256d(hdr), "little") <= easy_target_int:
                        found_nonce = nonce
                        break
                self.assertIsNotNone(found_nonce)

                await send(
                    {
                        "id": 4,
                        "method": "mining.submit",
                        "params": ["bitaxe.rig1", job_id, en2_hex, ntime_hex, f"{found_nonce:08x}"],
                    }
                )
                # Server may broadcast new job first or submit response first
                submit_resp = None
                for _ in range(4):
                    msg = await recv()
                    if msg.get("id") == 4:
                        submit_resp = msg
                        break
                self.assertIsNotNone(submit_resp)
                self.assertTrue(submit_resp["result"])
                self.assertEqual(len(fake_rpc.submitted), 1)
                self.assertEqual(fake_rpc.submitted[0][0], "aa" * 32)
                self.assertEqual(srv.aux_blocks_accepted, 1)
                self.assertEqual(srv.shares_accepted, 1)

                # 5. Replay same share with uppercase hex -> MUST be rejected as duplicate (code 22)
                await send(
                    {
                        "id": 5,
                        "method": "mining.submit",
                        "params": [
                            "bitaxe.rig1",
                            job_id,
                            en2_hex.upper(),
                            ntime_hex.upper(),
                            f"{found_nonce:08X}",
                        ],
                    }
                )
                dup_resp = None
                for _ in range(4):
                    msg = await recv()
                    if msg.get("id") == 5:
                        dup_resp = msg
                        break
                self.assertIsNotNone(dup_resp)
                self.assertFalse(dup_resp["result"])
                self.assertEqual(dup_resp["error"][0], 22)

                # 6. Malformed version-rolling mask & non-dict JSON frame handled safely without disconnect
                await send(
                    {
                        "id": 6,
                        "method": "mining.configure",
                        "params": [["version-rolling"], {"version-rolling.mask": "not_hex"}],
                    }
                )
                bad_cfg = await recv()
                self.assertEqual(bad_cfg["id"], 6)
                self.assertIsNotNone(bad_cfg["error"])

                writer.close()
                await writer.wait_closed()

                # 7. Oversized line (> MAX_LINE_BYTES) drops client cleanly without crashing server
                r2, w2 = await asyncio.open_connection("127.0.0.1", port)
                w2.write(b"A" * (sa.MAX_LINE_BYTES + 256) + b"\n")
                await w2.drain()
                eof_data = await asyncio.wait_for(r2.read(16), timeout=5.0)
                self.assertEqual(eof_data, b"")
                w2.close()
                await w2.wait_closed()
            finally:
                tcp_server.close()
                await tcp_server.wait_closed()

        asyncio.run(run_flow())


    def test_password_diff_and_suggest_difficulty(self) -> None:
        self.assertEqual(sa.parse_password_difficulty("d=4096"), 4096.0)
        self.assertEqual(sa.parse_password_difficulty("x,d=65536"), 65536.0)
        self.assertIsNone(sa.parse_password_difficulty("x"))
        self.assertIsNone(sa.parse_password_difficulty("d=-5"))

        async def run_diff_flow() -> None:
            # High network difficulty (~100,000) so suggested_diff=4096 is honored directly
            hard_target_int = sa.difficulty_to_target(100_000.0)
            fake_rpc = FakeAuxRpc(hard_target_int.to_bytes(32, "little").hex())
            srv = sa.AuxPowStratumServer(ns_rpc=fake_rpc, poll_interval=60.0)
            srv.refresh_template_sync()
            tcp_server = await asyncio.start_server(srv.handle_stratum_client, "127.0.0.1", 0)
            port = tcp_server.sockets[0].getsockname()[1]
            try:
                reader, writer = await asyncio.open_connection("127.0.0.1", port)

                async def send(obj):
                    writer.write((json.dumps(obj) + "\n").encode("utf-8"))
                    await writer.drain()

                async def recv():
                    line = await asyncio.wait_for(reader.readline(), timeout=5.0)
                    return json.loads(line.decode("utf-8"))

                await send({"id": 1, "method": "mining.subscribe", "params": ["mrr-test/1.0"]})
                sub = await recv()
                self.assertEqual(sub["id"], 1)

                await send({"id": 2, "method": "mining.suggest_difficulty", "params": [4096]})
                res2 = await recv()
                self.assertTrue(res2["result"])

                await send({"id": 3, "method": "mining.authorize", "params": ["rig1", "x,d=16384"]})
                res3 = await recv()
                self.assertTrue(res3["result"])
                d2 = await recv()
                self.assertEqual(d2["method"], "mining.set_difficulty")
                self.assertEqual(d2["params"][0], 16384)
                self.assertIsInstance(d2["params"][0], int)
                n2 = await recv()
                self.assertEqual(n2["method"], "mining.notify")

                writer.close()
                await writer.wait_closed()
            finally:
                tcp_server.close()
                await tcp_server.wait_closed()

        asyncio.run(run_diff_flow())

    def test_no_clean_jobs_churn_on_same_height_ntime_bump(self) -> None:
        """Regression test for WhatsMiner 0.00 H/s bug: nTime/aux_hash changes at same height must NOT churn jobs."""
        easy_target_int = 0x7FFFFF << (8 * 29)
        fake_rpc = FakeAuxRpc(easy_target_int.to_bytes(32, "little").hex())
        srv = sa.AuxPowStratumServer(ns_rpc=fake_rpc, poll_interval=60.0)

        job1 = srv.refresh_template_sync()
        self.assertIsNotNone(job1)
        self.assertTrue(job1.clean_jobs)

        # Simulate createauxblock returning a different aux_hash (nTime bumped) at the SAME height & bits
        fake_rpc.aux_hash = "cc" * 32
        job_suppressed = srv.refresh_template_sync()
        self.assertIsNone(job_suppressed, "Same-height nTime bump within 30s must NOT broadcast a new job")

        # Simulate 31s keepalive expiry at SAME height -> refresh happens with clean_jobs=False
        job1.created_at -= 31.0
        job_keepalive = srv.refresh_template_sync()
        self.assertIsNotNone(job_keepalive)
        self.assertFalse(job_keepalive.clean_jobs, "Keepalive refresh at same height must use clean_jobs=False")

        # Simulate real new block height -> MUST fire immediately with clean_jobs=True
        fake_rpc.height += 1
        fake_rpc.aux_hash = "dd" * 32
        job_new_tip = srv.refresh_template_sync()
        self.assertIsNotNone(job_new_tip)
        self.assertTrue(job_new_tip.clean_jobs, "New chain tip height must trigger clean_jobs=True immediately")

    def test_canonical_coinbase_and_merkle_branch_for_asic_compat(self) -> None:
        """Regression test for WhatsMiner/Antminer S21 ASIC Stratum parsing (CKPool-canonical wire format)."""
        self.assertEqual(sa.EXTRANONCE2_SIZE, 8)
        easy_target_int = 0x7FFFFF << (8 * 29)
        fake_rpc = FakeAuxRpc(easy_target_int.to_bytes(32, "little").hex())
        srv = sa.AuxPowStratumServer(ns_rpc=fake_rpc, poll_interval=60.0)
        job = srv.refresh_template_sync()
        self.assertIsNotNone(job)

        # Compact coinb1 (< 64 bytes) avoids midstate split bugs on strict ASIC firmwares; nVersion == 1
        self.assertLess(len(job.coinb1), 64)
        self.assertEqual(struct.unpack("<I", job.coinb1[:4])[0], 1)
        # Namecoin AuxPoW magic commitment lives cleanly in coinb2
        self.assertIn(sa.MERGED_MINING_MAGIC, job.coinb2)
        # Canonical Bitcoin nbits, non-zero prevhash, and non-empty 1-sibling merkle_branch
        self.assertEqual(job.parent_nbits, 0x1701F0A3)
        self.assertEqual(len(job.merkle_branch_internal), 1)
        prev_display = sa.stratum_prevhash_to_internal(job.parent_prevhash_stratum)[::-1].hex()
        self.assertTrue(prev_display.startswith("00000000000000000001"))

    def test_mrr_handshake_fixed_diff_overrides_suggest_and_low_aux_diff(self) -> None:
        """Regression test: explicit password d=524288 must beat suggest_difficulty AND low launch aux_diff."""
        async def run_mrr() -> None:
            # Very low network diff (~1.0) to prove fixed_diff=524288 is NOT clamped down to aux_diff
            low_target_int = sa.difficulty_to_target(1.0)
            fake_rpc = FakeAuxRpc(low_target_int.to_bytes(32, "little").hex())
            srv = sa.AuxPowStratumServer(ns_rpc=fake_rpc, poll_interval=60.0)
            srv.refresh_template_sync()
            tcp_server = await asyncio.start_server(srv.handle_stratum_client, "127.0.0.1", 0)
            port = tcp_server.sockets[0].getsockname()[1]
            try:
                reader, writer = await asyncio.open_connection("127.0.0.1", port)

                async def send(obj):
                    writer.write((json.dumps(obj) + "\n").encode("utf-8"))
                    await writer.drain()

                async def recv():
                    line = await asyncio.wait_for(reader.readline(), timeout=5.0)
                    return json.loads(line.decode("utf-8"))

                await send({"id": 1, "method": "mining.subscribe", "params": ["MiningRigRentals/Test/1.0"]})
                sub = await recv()
                self.assertEqual(sub["id"], 1)
                self.assertEqual(sub["result"][2], 8)

                await send({"id": 2, "method": "mining.extranonce.subscribe", "params": []})
                ex = await recv()
                self.assertEqual(ex["id"], 2)
                self.assertTrue(ex["result"])

                await send({"id": 3, "method": "mining.authorize", "params": ["ns1testworker.s21", "d=524288"]})
                auth = await recv()
                self.assertTrue(auth["result"])
                diff_msg = await recv()
                self.assertEqual(diff_msg["method"], "mining.set_difficulty")
                self.assertEqual(diff_msg["params"][0], 524288)
                self.assertIsInstance(diff_msg["params"][0], int)
                notify = await recv()
                self.assertEqual(notify["method"], "mining.notify")

                # Subsequent suggest_difficulty(16384) must NOT downgrade explicit password d=524288
                await send({"id": 4, "method": "mining.suggest_difficulty", "params": [16384]})
                sug = await recv()
                self.assertEqual(sug["id"], 4)
                self.assertTrue(sug["result"])
                client = next(iter(srv.clients.values()))
                self.assertEqual(client["difficulty"], 524288.0)

                writer.close()
                await writer.wait_closed()
            finally:
                tcp_server.close()
                await tcp_server.wait_closed()

        asyncio.run(run_mrr())

    def test_suggest_difficulty_sends_followup_notify_and_grace_window(self) -> None:
        """Regression test: post-auth suggest_difficulty sends set_difficulty + notify(clean=False) + grace window."""
        async def run_sug() -> None:
            hard_target_int = sa.difficulty_to_target(250_000.0)
            fake_rpc = FakeAuxRpc(hard_target_int.to_bytes(32, "little").hex())
            srv = sa.AuxPowStratumServer(ns_rpc=fake_rpc, poll_interval=60.0)
            srv.refresh_template_sync()
            tcp_server = await asyncio.start_server(srv.handle_stratum_client, "127.0.0.1", 0)
            port = tcp_server.sockets[0].getsockname()[1]
            try:
                reader, writer = await asyncio.open_connection("127.0.0.1", port)

                async def send(obj):
                    writer.write((json.dumps(obj) + "\n").encode("utf-8"))
                    await writer.drain()

                async def recv():
                    line = await asyncio.wait_for(reader.readline(), timeout=5.0)
                    return json.loads(line.decode("utf-8"))

                await send({"id": 1, "method": "mining.subscribe", "params": ["whatsminer/v1.1"]})
                _ = await recv()
                await send({"id": 2, "method": "mining.authorize", "params": ["rig.wm", "x"]})
                _ = await recv()  # auth
                _ = await recv()  # initial set_difficulty
                _ = await recv()  # initial notify

                await send({"id": 3, "method": "mining.suggest_difficulty", "params": [65536]})
                m1 = await recv()
                m2 = await recv()
                m3 = await recv()
                methods = [m.get("method") for m in (m1, m2, m3) if "method" in m]
                self.assertIn("mining.set_difficulty", methods)
                self.assertIn("mining.notify", methods)
                notify_msg = next(m for m in (m1, m2, m3) if m.get("method") == "mining.notify")
                self.assertFalse(notify_msg["params"][8], "Follow-up notify after suggest_difficulty must have clean_jobs=False")
                client = next(iter(srv.clients.values()))
                self.assertEqual(client["difficulty"], 65536.0)
                self.assertIsNotNone(client.get("prev_difficulty"))

                writer.close()
                await writer.wait_closed()
            finally:
                tcp_server.close()
                await tcp_server.wait_closed()

        asyncio.run(run_sug())

    def test_block_acceptance_broadcasts_clean_jobs_to_all_payout_keys(self) -> None:
        """Regression test: when Worker A wins a block, Worker B on a different payout key gets clean_jobs=True immediately."""
        async def run_multi() -> None:
            easy_target_int = 0x7FFFFF << (8 * 29)
            fake_rpc = FakeAuxRpc(easy_target_int.to_bytes(32, "little").hex())
            srv = sa.AuxPowStratumServer(ns_rpc=fake_rpc, poll_interval=60.0)
            srv.refresh_template_sync()
            tcp_server = await asyncio.start_server(srv.handle_stratum_client, "127.0.0.1", 0)
            port = tcp_server.sockets[0].getsockname()[1]
            addr_a = "ns1" + ("a" * 71)
            addr_b = "ns1" + ("b" * 71)
            try:
                r_a, w_a = await asyncio.open_connection("127.0.0.1", port)
                r_b, w_b = await asyncio.open_connection("127.0.0.1", port)

                async def send(w, obj):
                    w.write((json.dumps(obj) + "\n").encode("utf-8"))
                    await w.drain()

                async def recv(r):
                    line = await asyncio.wait_for(r.readline(), timeout=5.0)
                    return json.loads(line.decode("utf-8"))

                await send(w_a, {"id": 1, "method": "mining.subscribe", "params": ["minerA"]})
                sub_a = await recv(r_a)
                en1_a = sub_a["result"][1]
                await send(w_a, {"id": 2, "method": "mining.authorize", "params": [f"{addr_a}.rigA", "x"]})
                _ = await recv(r_a)
                _ = await recv(r_a)
                notify_a = await recv(r_a)

                await send(w_b, {"id": 1, "method": "mining.subscribe", "params": ["minerB"]})
                _ = await recv(r_b)
                await send(w_b, {"id": 2, "method": "mining.authorize", "params": [f"{addr_b}.rigB", "x"]})
                _ = await recv(r_b)
                _ = await recv(r_b)
                _ = await recv(r_b)

                job_id, prevhash, coinb1_hex, coinb2_hex, branch_hex, ver_hex, nbits_hex, ntime_hex, _ = notify_a["params"]
                en2_hex = "00" * (sa.EXTRANONCE2_SIZE - 1) + "01"
                coinbase = bytes.fromhex(coinb1_hex + en1_a + en2_hex + coinb2_hex)
                merkle_root = sa.fold_merkle_root(sa.sha256d(coinbase), [bytes.fromhex(h) for h in branch_hex])
                prev_internal = sa.stratum_prevhash_to_internal(prevhash)
                ver_u32, nbits_u32, ntime_u32 = int(ver_hex, 16), int(nbits_hex, 16), int(ntime_hex, 16)

                found_nonce = None
                for nonce in range(100_000):
                    hdr = struct.pack("<I", ver_u32) + prev_internal + merkle_root + struct.pack("<III", ntime_u32, nbits_u32, nonce)
                    if int.from_bytes(sa.sha256d(hdr), "little") <= easy_target_int:
                        found_nonce = nonce
                        break
                self.assertIsNotNone(found_nonce)

                await send(w_a, {"id": 3, "method": "mining.submit", "params": [f"{addr_a}.rigA", job_id, en2_hex, ntime_hex, f"{found_nonce:08x}"]})
                # Worker B MUST immediately receive clean_jobs=True for height 2 (possibly preceded by set_difficulty)!
                b_notify = None
                for _ in range(3):
                    msg = await recv(r_b)
                    if msg.get("method") == "mining.notify":
                        b_notify = msg
                        break
                self.assertIsNotNone(b_notify)
                self.assertTrue(b_notify["params"][8], "Other payout workers must immediately receive clean_jobs=True on block win")

                w_a.close()
                w_b.close()
                await w_a.wait_closed()
                await w_b.wait_closed()
            finally:
                tcp_server.close()
                await tcp_server.wait_closed()

        asyncio.run(run_multi())


if __name__ == "__main__":
    unittest.main()
