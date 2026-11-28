import hashlib
import unittest

import erc7683_solver_daemon as solver


class Erc7683SolverDaemonTests(unittest.TestCase):
    def setUp(self):
        self.preimage = hashlib.sha256(b"solver-test-preimage").hexdigest()
        self.payment_hash = hashlib.sha256(bytes.fromhex(self.preimage)).hexdigest()
        self.sample_order = {
            "origin_chain": "base",
            "payment_hash_sha256": f"0x{self.payment_hash}",
            "eip712": {
                "typed_data": {
                    "message": {
                        "openDeadline": 1_800_000_000,
                        "fillDeadline": 1_800_007_200,
                    }
                }
            },
            "order_data_decoded": {
                "inputAmount": "250000000",  # $250.00 USDC
                "outputAmountSats": 100_000_000,  # 1.00 LOVE -> $250/LOVE
            },
        }

    def test_accepts_order_above_reserve_price_and_rejects_below_floor(self):
        ok_policy = solver.SolverPolicy(min_reserve_price_usd=100.0)
        res_ok = solver.evaluate_erc7683_order(self.sample_order, ok_policy)
        self.assertTrue(res_ok["fillable"])
        self.assertEqual(res_ok["implied_usd_per_love"], 250.0)

        strict_policy = solver.SolverPolicy(min_reserve_price_usd=500.0)
        res_reject = solver.evaluate_erc7683_order(self.sample_order, strict_policy)
        self.assertFalse(res_reject["fillable"])
        self.assertTrue(any("reserve floor" in r for r in res_reject["rejection_reasons"]))

    def test_rejects_insufficient_timelock_margin(self):
        tight_order = json_copy = dict(self.sample_order)
        tight_order["eip712"] = {
            "typed_data": {"message": {"openDeadline": 1_800_000_000, "fillDeadline": 1_800_000_600}}
        }
        res = solver.evaluate_erc7683_order(tight_order, solver.SolverPolicy(min_reserve_price_usd=10.0))
        self.assertFalse(res["fillable"])
        self.assertTrue(any("timelock margin" in r for r in res["rejection_reasons"]))

    def test_preimage_verification_and_multirail_claim_commands(self):
        self.assertTrue(solver.verify_preimage_constant_time(self.preimage, self.payment_hash))
        self.assertFalse(solver.verify_preimage_constant_time("00" * 32, self.payment_hash))

        out = solver.emit_multirail_claim_commands(self.preimage, self.payment_hash, "11" * 32)
        self.assertTrue(out["verified"])
        for rail in (
            "evm_erc7683_settler",
            "evm_newsatoshi_htlc",
            "newsatoshi_mainnet_v2",
            "cosmos_ibc_cosmwasm",
            "sui_move",
            "aptos_move",
        ):
            self.assertIn(rail, out["commands"])
            self.assertIn(self.preimage, out["commands"][rail])

        with self.assertRaises(ValueError):
            solver.emit_multirail_claim_commands("00" * 32, self.payment_hash, "11" * 32)


if __name__ == "__main__":
    unittest.main()
