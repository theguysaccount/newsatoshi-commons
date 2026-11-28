#!/usr/bin/env node
import {
  getInteropMatrix,
  compileErc7683Intent,
  compileUniversalHtlc,
  compileIso20022Bridge,
  simulateUniversalInterop,
} from "../../sdk/js/newsatoshi-402.mjs";

const matrix = await getInteropMatrix();
console.log("1. Live Mainnet v2 Tip:", matrix.newsatoshi_mainnet_v2.height);
console.log(
  "   Live EVMs:",
  matrix.evm_rails.map((e) => `${e.slug}=${e.live ? `#${e.latest_block}` : "OFFLINE"}`).join(" | ")
);

const intent = await compileErc7683Intent({
  origin_chain: "base",
  user_evm_address: "0x1111222233334444555566667777888899990000",
  input_token: "USDC",
  input_amount_units: 250000000,
  output_asset: "LOVE",
  output_amount_sats: 100000000,
  destination_ns1: "ns1qquickstart000000000000000000001",
});
console.log("2. ERC-7683 EIP-712 Digest:", intent.eip712.signing_digest);

const htlc = await compileUniversalHtlc({
  source_rail: "sui_move",
  dest_rail: "cosmos_ibc",
  amount_display: "500 USDC",
  recipient: "cosmos1quickstart0000000000000000",
});
console.log("3. Universal HTLC Bundle:", htlc.bundle_id_sha256, `(${htlc.source_leg.rail} -> ${htlc.destination_leg.rail})`);

const iso = await compileIso20022Bridge({
  debtor_bic: "CHASUS33XXX",
  creditor_bic: "FRBKUS44XXX",
  currency: "USD",
  amount_decimal: "250000.00",
  stablecoin_ticker: "BANKUSD",
});
console.log("4. ISO 20022 pacs.008:", iso.msg_id, "asset_id:", iso.rfc0028_stablecoin.asset_id, "verified:", iso.roundtrip_verified);

const sim = await simulateUniversalInterop();
console.log(
  "5. 4-Domain STARK Proof:",
  "ok=", sim.ok,
  "stark_valid=", sim.domain_4_ai_agent_and_stark_finality.stark_valid,
  "verify_ms=", sim.domain_4_ai_agent_and_stark_finality.stark_verify_ms
);
