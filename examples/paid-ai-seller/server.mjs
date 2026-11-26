#!/usr/bin/env node
/**
 * Newsatoshi Paid AI API / GPU Seller Turnkey Proxy (`RFC-0022` + `RFC-0024` + `x402 v2`)
 *
 * Wraps ANY local GPU server (Ollama at localhost:11434, vLLM at localhost:8000, llama.cpp)
 * or upstream OpenAI-compatible API and monetizes every request in native shielded LOVE
 * via O(1) pure-SHA-256 PayWord vouchers anchored to unilateral RFC-0022 L1 channels!
 *
 * Features:
 *   - Advertises seller's RFC-0024 L1 Trust Bond via `X-Newsatoshi-Seller-Bond`
 *   - Persists highest accepted voucher per channel to disk (survives restarts)
 *   - Exports ready-to-submit unilateral RFC-0022 `ChannelClaim` vouchers even if buyer disappears
 *   - Zero external npm dependencies (Node 18+)
 */
import http from "node:http";
import path from "node:path";
import os from "node:os";
import {
  createDual402Gate,
  createFilePayWordStore,
} from "../../sdk/js/newsatoshi-402.mjs";

export async function createPaidAiSellerServer({
  upstreamBaseUrl = process.env.UPSTREAM_AI_URL || "http://127.0.0.1:11434",
  recipientAddress = process.env.NS_SELLER_ADDRESS || "nsd1seller00000000000000000000000000000000000000000000000000000",
  sellerBondIdHex = process.env.NS_SELLER_BOND_ID || "",
  unitPriceBaseUnits = Number(process.env.NS_PRICE_BASE_UNITS || 10_000),
  requireFunding = process.env.NS_REQUIRE_FUNDING === "1",
  seedBaseUrl = process.env.NS_SEED_URL || null,
  stateFile = process.env.NS_WATERMARK_FILE || path.join(os.tmpdir(), "newsatoshi-paid-ai-watermarks.json"),
  adminToken = process.env.NS_SELLER_ADMIN_TOKEN || "",
  fetchImpl = globalThis.fetch,
} = {}) {
  const store = await createFilePayWordStore(stateFile);
  const gate = createDual402Gate({
    recipient: recipientAddress,
    unitPriceBaseUnits,
    amountLove: (unitPriceBaseUnits / 1e8).toFixed(8),
    requireFunding,
    allowUnfundedDemo: !requireFunding,
    store,
  });

  const earnedByTip = new Map();

  const server = http.createServer(async (req, res) => {
    const urlPath = (req.url || "/").split("?")[0];

    if (req.method === "GET" && urlPath === "/health") {
      res.writeHead(200, { "Content-Type": "application/json" });
      res.end(
        JSON.stringify({
          ok: true,
          service: "newsatoshi-paid-ai-seller",
          upstream: upstreamBaseUrl,
          recipient_address: recipientAddress,
          seller_bond_id_hex: sellerBondIdHex || null,
          unit_price_base_units: unitPriceBaseUnits,
          require_funding: requireFunding,
          active_channels_tracked: earnedByTip.size,
        })
      );
      return;
    }

    // Authenticated seller admin endpoint: export highest accepted voucher per channel for unilateral L1 ChannelClaim
    if (req.method === "GET" && urlPath === "/admin/vouchers") {
      const auth = req.headers["authorization"] || "";
      if (!adminToken || auth !== `Bearer ${adminToken}`) {
        res.writeHead(401, { "Content-Type": "application/json" });
        res.end(JSON.stringify({ ok: false, error: "unauthorized" }));
        return;
      }
      res.writeHead(200, { "Content-Type": "application/json" });
      res.end(JSON.stringify({ ok: true, vouchers: Array.from(earnedByTip.values()) }));
      return;
    }

    // Gate all inference / API calls with HTTP 402 PayWord verification
    const hdr = req.headers["x-newsatoshi-payword"];
    const check = await gate.verifyRequest(typeof hdr === "string" ? hdr : "", urlPath);

    const commonHeaders = {
      "Content-Type": "application/json",
      "X-Newsatoshi-Recipient": recipientAddress,
    };
    if (sellerBondIdHex) {
      commonHeaders["X-Newsatoshi-Seller-Bond"] = sellerBondIdHex;
    }

    if (!check.paid) {
      res.writeHead(402, {
        ...commonHeaders,
        "WWW-Authenticate": check.challenge.wwwAuthenticate,
      });
      res.end(JSON.stringify({ ok: false, reason: check.reason, challenge: check.challenge }));
      return;
    }

    earnedByTip.set(check.tipHex, {
      tip_hex: check.tipHex,
      channel_id: check.channelId,
      highest_ticks: check.ticks,
      last_token_hex: check.tokenHex,
      total_earned_base_units: check.totalPaidBaseUnits,
      collectible_on_chain: check.collectibleOnChain,
    });

    // Forward paid request verbatim to local GPU / upstream LLM API
    const chunks = [];
    for await (const c of req) chunks.push(c);
    const bodyBuf = Buffer.concat(chunks);
    const upstreamUrl = `${upstreamBaseUrl.replace(/\/+$/, "")}${req.url || "/"}`;

    try {
      const upResp = await fetchImpl(upstreamUrl, {
        method: req.method,
        headers: {
          "Content-Type": req.headers["content-type"] || "application/json",
          Accept: req.headers["accept"] || "application/json",
        },
        body: req.method === "GET" || req.method === "HEAD" ? undefined : bodyBuf,
      });
      const upText = await upResp.text();
      res.writeHead(upResp.status, {
        ...commonHeaders,
        "X-Newsatoshi-Ticks-Accepted": String(check.ticks),
        "X-Newsatoshi-Hashes-Evaluated": String(check.hashesEvaluated),
        "X-Newsatoshi-Funding-Status": check.fundingStatus,
      });
      res.end(upText);
    } catch (err) {
      res.writeHead(502, commonHeaders);
      res.end(JSON.stringify({ ok: false, error: `upstream AI backend error: ${err.message}` }));
    }
  });

  return { server, gate, earnedByTip };
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const port = Number(process.env.PORT || 8402);
  const { server } = await createPaidAiSellerServer();
  server.listen(port, "127.0.0.1", () => {
    console.log(`Newsatoshi Paid AI/GPU Seller Proxy listening on http://127.0.0.1:${port}`);
  });
}
