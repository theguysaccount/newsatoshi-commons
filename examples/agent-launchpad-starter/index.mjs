#!/usr/bin/env node
/**
 * Newsatoshi Decentralized Agent Swarm Launchpad Starter Kit (RFC-0027 + RFC-0024)
 *
 * Launch a `pequalnp.money`-style decentralized AI agent mission + zero-rugpull bonding-curve
 * launchpad on Newsatoshi in <95 lines of zero-dependency JavaScript:
 *   1. Launches a mission charter (`80% outside contributors / 20% public goods`, `0% founder cut`)
 *   2. Simulates a launchpad bonding-curve buy -> routes 5% fee straight into verified-work bounties
 *   3. Orchestrator posts a Lean 4 lemma subtask backed by the bounty pool
 *   4. External agent (Claude Code / Cursor / Gemini CLI) submits verified proof artifact digest
 *   5. Computes exact L1 RFC-0027 `post_task_message` & `claim_task_message` digests + 80/20 split!
 */
import crypto from "node:crypto";
import {
  quoteMissionCurveBuy,
  quoteMissionCurveRedeem,
  missionPostTaskMessageHex,
  missionClaimTaskMessageHex,
} from "../../sdk/js/newsatoshi-402.mjs";

const sha256Hex = (s) => crypto.createHash("sha256").update(s).digest("hex");

// 1. Mission charter (80% verified outside contributors, 20% Lean FRO, 5% trade fee -> bounties)
const mission = {
  name: "Open P vs NP Lean 4 Swarm",
  ticker: "PVNP",
  verifierKind: "lean4_zero_sorry",
  contributorBps: 8_000,
  publicGoodsBps: 2_000,
  curveFeeBps: 500,
  bountyPool: 200_000_000, // 2 LOVE seed
  curveReserve: 0,
  totalShares: 0,
};
const missionIdHex = sha256Hex(`nsMISS:${mission.name}:${mission.ticker}`);

// 2. Launchpad buyer deposits 20 LOVE -> 1 LOVE (5%) flows automatically into verified-work bounties!
const buy = quoteMissionCurveBuy({
  curveReserve: mission.curveReserve,
  totalShares: mission.totalShares,
  curveFeeBps: mission.curveFeeBps,
  amount: 2_000_000_000,
});
mission.bountyPool += buy.feeToBountyPool;
mission.curveReserve = buy.newCurveReserve;
mission.totalShares = buy.newTotalShares;

// 3. Orchestrator decomposes mission into Lemma #1 backed by the bounty pool
const taskId = 1;
const specHashHex = sha256Hex("lemma-01-relativization-barrier.lean");
const reward = 200_010_000; // 2.0001 LOVE (includes 10,000 miner fee)
const postDigestHex = await missionPostTaskMessageHex(missionIdHex, taskId, specHashHex, reward);

// 4. External AI agent solves lemma with 0 sorry; checker desk signs exact L1 claim digest
const proofHashHex = sha256Hex("lean4-4.36.0-build-0-sorry-verified");
const contributorApkHex = sha256Hex("external-agent-shielded-apk");
const minerFee = 10_000;
const claimDigestHex = await missionClaimTaskMessageHex(
  missionIdHex,
  taskId,
  specHashHex,
  proofHashHex,
  contributorApkHex,
  minerFee
);

const netReward = reward - minerFee;
const publicGoodsPayout = Math.floor((netReward * mission.publicGoodsBps) / 10_000);
const contributorPayout = netReward - publicGoodsPayout;
mission.bountyPool -= reward;

// 5. Launchpad share redemption routes ANOTHER 5% exit fee straight into the bounty pool!
const redeem = quoteMissionCurveRedeem({
  curveReserve: mission.curveReserve,
  totalShares: mission.totalShares,
  curveFeeBps: mission.curveFeeBps,
  shares: Math.floor(buy.sharesMinted / 2),
  minerFee: 5_000,
});

console.log(JSON.stringify({
  ok: true,
  rfc: "RFC-0027",
  mission_id_hex: missionIdHex,
  bonding_curve_buy: buy,
  task_posted: { task_id: taskId, reward, post_digest_hex: postDigestHex },
  verified_claim_split: {
    claim_digest_hex: claimDigestHex,
    contributor_payout_base_units: contributorPayout,
    public_goods_payout_base_units: publicGoodsPayout,
    miner_fee_base_units: minerFee,
    founder_cut_base_units: 0,
    conservation_exact: contributorPayout + publicGoodsPayout + minerFee === reward,
  },
  bonding_curve_redeem_feeds_bounty_pool: redeem,
  remaining_bounty_pool_after_redeem_fee: mission.bountyPool + redeem.feeToBountyPool,
}, null, 2));
