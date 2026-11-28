#!/usr/bin/env python3
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[2] / "sdk" / "python"))
import newsatoshi_402 as ns

matrix = ns.get_interop_matrix()
print("1. Live Mainnet v2 Tip:", matrix["newsatoshi_mainnet_v2"]["height"])
print(
    "   Live EVMs:",
    " | ".join(f"{e['slug']}=#{e['latest_block']}" if e["live"] else f"{e['slug']}=OFFLINE" for e in matrix["evm_rails"]),
)

intent = ns.compile_erc7683_intent(
    {
        "origin_chain": "optimism",
        "user_evm_address": "0x1111222233334444555566667777888899990000",
        "input_token": "USDC",
        "input_amount_units": 250000000,
        "output_asset": "LOVE",
        "output_amount_sats": 100000000,
        "destination_ns1": "ns1qquickstart000000000000000000001",
    }
)
print("2. ERC-7683 EIP-712 Digest:", intent["eip712"]["signing_digest"])

iso = ns.compile_iso20022_bridge({"stablecoin_ticker": "BANKUSD", "amount_decimal": "250000.00"})
print("3. ISO 20022 pacs.008:", iso["msg_id"], "verified:", iso["roundtrip_verified"])

sim = ns.simulate_universal_interop()
print("4. 4-Domain STARK Proof: ok=", sim["ok"], "stark_valid=", sim["domain_4_ai_agent_and_stark_finality"]["stark_valid"])
