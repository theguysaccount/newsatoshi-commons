#!/usr/bin/env node
/**
 * newsatoshi-paywall — 10-Second Zero-Code HTTP 402 PayWords Reverse Proxy (v0.36.0)
 *
 * Put in front of ANY existing HTTP API, Ollama/vLLM/OpenAI-compatible LLM server,
 * or Streamable HTTP MCP server to monetize every call at 0% protocol fee and $0
 * per-call gas via O(1) SHA-256 PayWords settled on Base, Optimism, or Arbitrum One.
 *
 * Usage:
 *   npx --yes -p github:theguysaccount/newsatoshi-commons newsatoshi-paywall \
 *     --upstream http://127.0.0.1:8000 \
 *     --pay-to 0xYOUR_EVM_ADDRESS \
 *     --chain base \
 *     --price-usdc 0.001 \
 *     --port 4020
 */
import crypto from "node:crypto";
import { spawn } from "node:child_process";
import fs from "node:fs";
import http from "node:http";
import https from "node:https";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const PAYWALL_CHAINS = Object.freeze({
  base: {
    slug: "base",
    label: "Base Mainnet",
    caip2: "eip155:8453",
    rpc: "https://mainnet.base.org",
    contract: "0x971BA814Df91c20DcA3aDd870C2B55d643965C50",
    usdc: "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913",
  },
  optimism: {
    slug: "optimism",
    label: "Optimism Mainnet",
    caip2: "eip155:10",
    rpc: "https://mainnet.optimism.io",
    contract: "0x3DeBB6d5ae490e6cF4F966585e000028111CF106",
    usdc: "0x0b2C639c533813f4Aa9D7837CAf62653d097Ff85",
  },
  arbitrum: {
    slug: "arbitrum",
    label: "Arbitrum One",
    caip2: "eip155:42161",
    rpc: "https://arb1.arbitrum.io/rpc",
    contract: "0x3DeBB6d5ae490e6cF4F966585e000028111CF106",
    usdc: "0xaf88d065e77c8cC2239327C5EDb3A432268e5831",
  },
});

const CHANNELS_SELECTOR = "0x35d99f35"; // channels(bytes32)

function sha256Buf(buf) {
  return crypto.createHash("sha256").update(buf).digest();
}

function normHex32(hexStr) {
  if (typeof hexStr !== "string") return null;
  const clean = hexStr.trim().replace(/^0x/i, "").toLowerCase();
  if (!/^[0-9a-f]{64}$/.test(clean)) return null;
  return clean;
}

function normAddr(addr) {
  if (typeof addr !== "string") return null;
  const clean = addr.trim();
  if (!/^0x[0-9a-fA-F]{40}$/.test(clean)) return null;
  return clean.toLowerCase();
}

function decodeAbiAddress(word64) {
  return ("0x" + word64.slice(24, 64)).toLowerCase();
}

function decodeAbiUint(word64) {
  return BigInt("0x" + word64);
}

/**
 * Live eth_call channels(bytes32) verification against Base / Optimism / Arbitrum RPC.
 * Struct layout of NewsatoshiPayWordChannel.Channel:
 *   0: payer (address)
 *   1: recipient (address)
 *   2: token (address)
 *   3: root (bytes32)
 *   4: unitPrice (uint256)
 *   5: maxCount (uint32)
 *   6: claimedCount (uint32)
 *   7: expiry (uint64)
 */
export async function verifyOnchainChannel(chain, channelIdHex, expectedPayTo, expectedRootHex, minUnitRaw, neededIndex, rpcUrlOverride) {
  const rpcUrl = rpcUrlOverride || chain.rpc;
  const data = CHANNELS_SELECTOR + channelIdHex;
  const resp = await fetch(rpcUrl, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      jsonrpc: "2.0",
      id: 1,
      method: "eth_call",
      params: [{ to: chain.contract, data }, "latest"],
    }),
    signal: AbortSignal.timeout(8000),
  });
  if (!resp.ok) throw new Error(`rpc_http_${resp.status}`);
  const j = await resp.json();
  if (j.error || typeof j.result !== "string") {
    throw new Error(`rpc_error: ${j.error?.message || "missing_result"}`);
  }
  const hex = j.result.replace(/^0x/i, "").toLowerCase();
  if (hex.length < 64 * 8) throw new Error("channel_not_found_or_invalid_abi");

  const w = (i) => hex.slice(i * 64, (i + 1) * 64);
  const payer = decodeAbiAddress(w(0));
  const recipient = decodeAbiAddress(w(1));
  const token = decodeAbiAddress(w(2));
  const root = w(3);
  const unitPrice = Number(decodeAbiUint(w(4)));
  const maxCount = Number(decodeAbiUint(w(5)));
  const claimedCount = Number(decodeAbiUint(w(6)));
  const expiry = Number(decodeAbiUint(w(7)));
  const nowSec = Math.floor(Date.now() / 1000);

  if (payer === "0x0000000000000000000000000000000000000000") return { ok: false, reason: "channel_not_opened_onchain" };
  if (recipient !== expectedPayTo.toLowerCase()) return { ok: false, reason: "channel_recipient_mismatch" };
  if (token !== chain.usdc.toLowerCase()) return { ok: false, reason: "channel_token_not_usdc" };
  if (root !== expectedRootHex) return { ok: false, reason: "channel_root_mismatch" };
  if (unitPrice < minUnitRaw) return { ok: false, reason: "channel_unit_price_too_low" };
  if (maxCount < neededIndex) return { ok: false, reason: "channel_max_steps_exceeded" };
  if (expiry <= nowSec + 60) return { ok: false, reason: "channel_expired_or_expiring" };

  return { ok: true, payer, recipient, token, root, unitPrice, maxCount, claimedCount, expiry };
}

function loadState(stateFile) {
  if (!stateFile) return { channels: {}, totalVerifiedCalls: 0, totalEarnedUsdcRaw: 0 };
  try {
    if (fs.existsSync(stateFile)) {
      const parsed = JSON.parse(fs.readFileSync(stateFile, "utf8"));
      return {
        channels: parsed.channels && typeof parsed.channels === "object" ? parsed.channels : {},
        totalVerifiedCalls: Number(parsed.totalVerifiedCalls || 0),
        totalEarnedUsdcRaw: Number(parsed.totalEarnedUsdcRaw || 0),
      };
    }
  } catch {
    // start clean if unreadable
  }
  return { channels: {}, totalVerifiedCalls: 0, totalEarnedUsdcRaw: 0 };
}

function persistState(stateFile, state) {
  if (!stateFile) return;
  const dir = path.dirname(stateFile);
  if (dir && dir !== ".") fs.mkdirSync(dir, { recursive: true });
  const tmp = `${stateFile}.tmp.${process.pid}`;
  fs.writeFileSync(tmp, JSON.stringify(state, null, 2), "utf8");
  fs.renameSync(tmp, stateFile);
}

function parseVoucherHeader(req) {
  const sig = req.headers["payment-signature"] || req.headers["x-payment"] || req.headers["x-newsatoshi-payword"];
  if (!sig || typeof sig !== "string") return null;
  const trimmed = sig.trim();
  try {
    if (trimmed.startsWith("{")) return JSON.parse(trimmed);
    return JSON.parse(Buffer.from(trimmed, "base64").toString("utf8"));
  } catch {
    return null;
  }
}

export function buildClaimCommands(chain, state, fairCurvePool) {
  const cmds = [];
  for (const [channelId, ch] of Object.entries(state.channels)) {
    if (!ch || ch.lastIndex <= 0) continue;
    const grossRaw = ch.lastIndex * ch.unitRaw;
    const entry = {
      channelId: "0x" + channelId,
      steps: ch.lastIndex,
      latestPreimage: "0x" + ch.lastToken,
      earnedUsdcRaw: grossRaw,
      earnedUsdcFormatted: (grossRaw / 1e6).toFixed(6),
      claimCommand: `cast send ${chain.contract} "claim(bytes32,uint32,bytes32)" 0x${channelId} ${ch.lastIndex} 0x${ch.lastToken} --rpc-url ${chain.rpc} --private-key $MERCHANT_KEY`,
    };
    if (fairCurvePool) {
      entry.injectRevenueCommands = [
        `cast send ${chain.usdc} "approve(address,uint256)" ${fairCurvePool} ${grossRaw} --rpc-url ${chain.rpc} --private-key $MERCHANT_KEY`,
        `cast send ${fairCurvePool} "injectRevenue(uint256)" ${grossRaw} --rpc-url ${chain.rpc} --private-key $MERCHANT_KEY`,
      ];
    }
    cmds.push(entry);
  }
  return cmds;
}

/**
 * Create a zero-dependency PayWords HTTP 402 reverse proxy server.
 */
export function createPaywallServer(options) {
  const upstreamUrl = new URL(options.upstream);
  const chain = PAYWALL_CHAINS[options.chain || "base"];
  if (!chain) throw new Error(`Unsupported chain '${options.chain}'. Use: base, optimism, arbitrum`);
  const payTo = normAddr(options.payTo);
  if (!payTo) throw new Error("Valid 0x EVM --pay-to address required");
  const priceUsdc = Number(options.priceUsdc ?? 0.001);
  if (!Number.isFinite(priceUsdc) || priceUsdc <= 0) throw new Error("--price-usdc must be > 0");
  const unitRaw = Math.max(1, Math.round(priceUsdc * 1e6));
  const maxStepsDefault = Math.max(1, Math.min(100000, Number(options.maxSteps || 1000)));
  const sandbox = Boolean(options.sandbox);
  const fairCurvePool = options.fairCurve ? normAddr(options.fairCurve) : null;
  const freePaths = new Set((options.freePaths || ["/health", "/.well-known/x402"]).map((p) => p.trim()));
  const stateFile = options.stateFile || null;
  const rpcUrl = options.rpcUrl || chain.rpc;
  const autoClaimThreshold = Number(options.autoClaimThreshold || 0);
  const autoClaimCmd = options.autoClaimCmd || "";
  const onAutoClaim = typeof options.onAutoClaim === "function" ? options.onAutoClaim : null;
  const inFlightClaims = new Set();
  const state = loadState(stateFile);

  function maybeTriggerAutoClaim(channelId, rec) {
    if (autoClaimThreshold <= 0) return;
    const settled = Number(rec.lastAutoClaimedIndex || 0);
    if (rec.lastIndex - settled < autoClaimThreshold) return;
    if (inFlightClaims.has(channelId)) return;
    const snapSteps = rec.lastIndex;
    const snapToken = rec.lastToken;
    const grossRaw = snapSteps * rec.unitRaw;
    const claimEvent = {
      chain: chain.slug,
      channelContract: chain.contract,
      channelId: "0x" + channelId,
      steps: snapSteps,
      latestPreimage: "0x" + snapToken,
      earnedUsdcRaw: grossRaw,
      rpcUrl,
      fairCurvePool,
    };
    inFlightClaims.add(channelId);
    rec.lastAutoClaimedIndex = snapSteps;
    persistState(stateFile, state);
    if (onAutoClaim) {
      Promise.resolve(onAutoClaim(claimEvent)).finally(() => inFlightClaims.delete(channelId));
      return;
    }
    if (autoClaimCmd) {
      const child = spawn(autoClaimCmd, {
        shell: true,
        stdio: "inherit",
        env: {
          ...process.env,
          PAYWALL_CHAIN: chain.slug,
          PAYWALL_CONTRACT: chain.contract,
          PAYWALL_RPC_URL: rpcUrl,
          PAYWALL_CHANNEL_ID: "0x" + channelId,
          PAYWALL_STEPS: String(snapSteps),
          PAYWALL_PREIMAGE: "0x" + snapToken,
          PAYWALL_EARNED_USDC_RAW: String(grossRaw),
          PAYWALL_FAIR_CURVE: fairCurvePool || "",
        },
      });
      child.on("exit", () => inFlightClaims.delete(channelId));
      child.on("error", () => inFlightClaims.delete(channelId));
    } else {
      inFlightClaims.delete(channelId);
    }
  }

  function challengePayload(reqPath) {
    return {
      x402Version: 2,
      scheme: "batch-settlement",
      binding: "paywords-sha256",
      error: "payment_required",
      resource: reqPath,
      network: chain.caip2,
      chain: chain.slug,
      asset: chain.usdc,
      assetSymbol: "USDC",
      channelContract: chain.contract,
      payTo,
      unitAmount: String(unitRaw),
      priceUsdc: (unitRaw / 1e6).toFixed(6),
      recommendedMaxSteps: maxStepsDefault,
      protocolFeePct: 0,
      sandboxMode: sandbox,
      sdk: {
        js: "npm install github:theguysaccount/newsatoshi-commons",
        python: 'pip install "git+https://github.com/theguysaccount/newsatoshi-commons.git#subdirectory=sdk/python"',
        docs: "https://newsatoshi.org/paywords/",
      },
    };
  }

  function sendJson(res, status, obj, extraHeaders = {}) {
    const body = JSON.stringify(obj, null, 2);
    res.writeHead(status, {
      "Content-Type": "application/json; charset=utf-8",
      "Content-Length": Buffer.byteLength(body),
      "Access-Control-Allow-Origin": "*",
      "Access-Control-Allow-Headers": "Content-Type, Authorization, PAYMENT-SIGNATURE, X-PAYMENT, X-Newsatoshi-PayWord",
      "Access-Control-Expose-Headers": "PAYMENT-REQUIRED, PAYMENT-RESPONSE, X-PayWord-Latency-Us",
      ...extraHeaders,
    });
    res.end(body);
  }

  function proxyToUpstream(req, res, receiptHeader, latencyUs) {
    const targetPath = req.url || "/";
    const isHttps = upstreamUrl.protocol === "https:";
    const transport = isHttps ? https : http;
    const headers = { ...req.headers, host: upstreamUrl.host };
    delete headers["payment-signature"];
    delete headers["x-payment"];
    delete headers["x-newsatoshi-payword"];

    const proxyReq = transport.request(
      {
        hostname: upstreamUrl.hostname,
        port: upstreamUrl.port || (isHttps ? 443 : 80),
        path: targetPath,
        method: req.method,
        headers,
      },
      (proxyRes) => {
        const outHeaders = { ...proxyRes.headers };
        if (receiptHeader) outHeaders["payment-response"] = receiptHeader;
        if (latencyUs !== null) outHeaders["x-payword-latency-us"] = String(latencyUs);
        outHeaders["access-control-expose-headers"] = "PAYMENT-REQUIRED, PAYMENT-RESPONSE, X-PayWord-Latency-Us";
        res.writeHead(proxyRes.statusCode || 502, outHeaders);
        proxyRes.pipe(res);
      }
    );
    proxyReq.on("error", (err) => {
      if (!res.headersSent) {
        sendJson(res, 502, { ok: false, error: "upstream_unreachable", detail: err.message });
      }
    });
    req.pipe(proxyReq);
  }

  const server = http.createServer(async (req, res) => {
    const reqUrl = new URL(req.url || "/", "http://localhost");
    const pathname = reqUrl.pathname;

    if (req.method === "OPTIONS") {
      res.writeHead(204, {
        "Access-Control-Allow-Origin": "*",
        "Access-Control-Allow-Methods": "GET,POST,PUT,PATCH,DELETE,OPTIONS",
        "Access-Control-Allow-Headers": "Content-Type, Authorization, PAYMENT-SIGNATURE, X-PAYMENT, X-Newsatoshi-PayWord",
      });
      res.end();
      return;
    }

    if (pathname === "/_paywall/status") {
      return sendJson(res, 200, {
        ok: true,
        proxy: "newsatoshi-paywall/0.36.0",
        upstream: upstreamUrl.origin,
        chain: chain.slug,
        caip2: chain.caip2,
        channelContract: chain.contract,
        payTo,
        unitUsdcRaw: unitRaw,
        priceUsdc: (unitRaw / 1e6).toFixed(6),
        sandboxMode: sandbox,
        fairCurvePool,
        activeChannels: Object.keys(state.channels).length,
        totalVerifiedCalls: state.totalVerifiedCalls,
        totalEarnedUsdcRaw: state.totalEarnedUsdcRaw,
        totalEarnedUsdcFormatted: (state.totalEarnedUsdcRaw / 1e6).toFixed(6),
      });
    }

    if (pathname === "/_paywall/claim-commands") {
      return sendJson(res, 200, {
        ok: true,
        chain: chain.slug,
        channelContract: chain.contract,
        claims: buildClaimCommands(chain, state, fairCurvePool),
      });
    }

    if (pathname === "/.well-known/x402") {
      return sendJson(res, 200, {
        x402Version: 2,
        schemes: ["batch-settlement"],
        bindings: ["paywords-sha256"],
        defaultPayTo: payTo,
        settlementChain: chain,
        priceUsdc: (unitRaw / 1e6).toFixed(6),
        protocolFeePct: 0,
      });
    }

    if (freePaths.has(pathname)) {
      return proxyToUpstream(req, res, null, null);
    }

    const voucher = parseVoucherHeader(req);
    if (!voucher) {
      const challenge = challengePayload(pathname);
      const b64Challenge = Buffer.from(JSON.stringify(challenge)).toString("base64");
      return sendJson(res, 402, challenge, { "PAYMENT-REQUIRED": b64Challenge });
    }

    const channelId = normHex32(voucher.channelId || voucher.channel_id);
    const tokenHex = normHex32(voucher.tokenHex || voucher.token_hex || voucher.token);
    const rootHex = normHex32(voucher.rootHex || voucher.root_hex || voucher.tip_hex);
    const index = Number(voucher.index ?? voucher.step ?? voucher.ticks);
    const reqMaxSteps = Number(voucher.maxSteps ?? voucher.max_steps ?? maxStepsDefault);

    if (!channelId || !tokenHex || !Number.isInteger(index) || index < 1) {
      return sendJson(res, 402, {
        ...challengePayload(pathname),
        error: "invalid_voucher_fields",
      });
    }

    let rec = state.channels[channelId];
    if (!rec) {
      if (!rootHex) {
        return sendJson(res, 402, { ...challengePayload(pathname), error: "missing_root_hex_on_first_call" });
      }
      let onchainStatus = "sandbox_unfunded_dev_mode";
      let verifiedMaxSteps = Math.max(1, Math.min(100000, Number.isInteger(reqMaxSteps) ? reqMaxSteps : maxStepsDefault));
      let verifiedUnitRaw = unitRaw;
      if (!sandbox) {
        try {
          const check = await verifyOnchainChannel(chain, channelId, payTo, rootHex, unitRaw, index, rpcUrl);
          if (!check.ok) {
            return sendJson(res, 402, { ...challengePayload(pathname), error: check.reason });
          }
          onchainStatus = "verified_onchain_funded";
          verifiedMaxSteps = check.maxCount;
          verifiedUnitRaw = check.unitPrice;
        } catch (err) {
          return sendJson(res, 402, { ...challengePayload(pathname), error: "onchain_rpc_verify_failed", detail: err.message });
        }
      }
      rec = {
        lastIndex: 0,
        lastToken: rootHex,
        rootHex,
        maxSteps: verifiedMaxSteps,
        unitRaw: verifiedUnitRaw,
        fundingStatus: onchainStatus,
      };
    }

    if (index <= rec.lastIndex) {
      return sendJson(res, 402, {
        ...challengePayload(pathname),
        error: "replay_rejected_monotonic_cursor",
        lastVerifiedIndex: rec.lastIndex,
        submittedIndex: index,
      });
    }
    if (index > rec.maxSteps) {
      return sendJson(res, 402, {
        ...challengePayload(pathname),
        error: "channel_exhausted_open_new_channel",
        maxSteps: rec.maxSteps,
      });
    }

    const delta = index - rec.lastIndex;
    if (delta > 5000) {
      return sendJson(res, 402, { ...challengePayload(pathname), error: "step_jump_too_large" });
    }

    const t0 = process.hrtime.bigint();
    let cur = Buffer.from(tokenHex, "hex");
    for (let i = 0; i < delta; i++) {
      cur = sha256Buf(cur);
    }
    const latencyUs = Math.max(1, Number((process.hrtime.bigint() - t0) / 1000n));

    if (cur.toString("hex") !== rec.lastToken) {
      return sendJson(res, 402, { ...challengePayload(pathname), error: "invalid_sha256_payword_proof" });
    }

    rec.lastIndex = index;
    rec.lastToken = tokenHex;
    state.channels[channelId] = rec;
    state.totalVerifiedCalls += 1;
    state.totalEarnedUsdcRaw += delta * rec.unitRaw;
    persistState(stateFile, state);
    maybeTriggerAutoClaim(channelId, rec);

    const receipt = {
      x402Version: 2,
      scheme: "batch-settlement",
      binding: "paywords-sha256",
      status: "accepted",
      network: chain.caip2,
      channelId: "0x" + channelId,
      verifiedStep: index,
      remainingSteps: rec.maxSteps - index,
      verifiedInUs: latencyUs,
      fundingStatus: rec.fundingStatus,
    };
    const receiptB64 = Buffer.from(JSON.stringify(receipt)).toString("base64");
    return proxyToUpstream(req, res, receiptB64, latencyUs);
  });

  return { server, state, chain };
}

function parseArgs(argv) {
  const opts = {
    upstream: process.env.PAYWALL_UPSTREAM || "",
    payTo: process.env.PAYWALL_PAY_TO || "",
    chain: process.env.PAYWALL_CHAIN || "base",
    priceUsdc: process.env.PAYWALL_PRICE_USDC || "0.001",
    port: Number(process.env.PAYWALL_PORT || 4020),
    host: process.env.PAYWALL_HOST || "0.0.0.0",
    maxSteps: Number(process.env.PAYWALL_MAX_STEPS || 1000),
    stateFile: process.env.PAYWALL_STATE_FILE || "./paywall-state.json",
    fairCurve: process.env.PAYWALL_FAIR_CURVE || "",
    rpcUrl: process.env.PAYWALL_RPC_URL || "",
    autoClaimThreshold: Number(process.env.PAYWALL_AUTO_CLAIM_THRESHOLD || 0),
    autoClaimCmd: process.env.PAYWALL_AUTO_CLAIM_CMD || "",
    sandbox: process.env.PAYWALL_SANDBOX === "1",
    freePaths: ["/health", "/.well-known/x402"],
    help: false,
  };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === "-h" || a === "--help") opts.help = true;
    else if (a === "--sandbox") opts.sandbox = true;
    else if (a === "--upstream") opts.upstream = argv[++i];
    else if (a === "--pay-to") opts.payTo = argv[++i];
    else if (a === "--chain") opts.chain = argv[++i];
    else if (a === "--price-usdc") opts.priceUsdc = argv[++i];
    else if (a === "--port") opts.port = Number(argv[++i]);
    else if (a === "--host") opts.host = argv[++i];
    else if (a === "--max-steps") opts.maxSteps = Number(argv[++i]);
    else if (a === "--state") opts.stateFile = argv[++i];
    else if (a === "--fair-curve") opts.fairCurve = argv[++i];
    else if (a === "--rpc-url") opts.rpcUrl = argv[++i];
    else if (a === "--auto-claim-threshold") opts.autoClaimThreshold = Number(argv[++i]);
    else if (a === "--auto-claim-cmd") opts.autoClaimCmd = argv[++i];
    else if (a === "--free-paths") opts.freePaths = argv[++i].split(",").filter(Boolean);
  }
  return opts;
}

const isMain = process.argv[1] && path.resolve(process.argv[1]) === path.resolve(fileURLToPath(import.meta.url));
if (isMain) {
  const opts = parseArgs(process.argv.slice(2));
  if (opts.help || !opts.upstream || !opts.payTo) {
    console.log(`newsatoshi-paywall v0.36.0 — 10-Second Zero-Code HTTP 402 PayWords Reverse Proxy

Usage:
  newsatoshi-paywall --upstream <URL> --pay-to <0xEVM_ADDRESS> [options]

Options:
  --upstream <url>        Target REST API, LLM server, or Streamable HTTP MCP server (required)
  --pay-to <0x...>        Recipient EVM address receiving 100% of USDC claims (required)
  --chain <slug>          Settlement L2: base (default), optimism, or arbitrum
  --price-usdc <num>      USDC price per API call (default: 0.001)
  --port <num>            Listen port (default: 4020)
  --state <file>          Persistent replay-guard JSON path (default: ./paywall-state.json)
  --fair-curve <0x...>    Optional Base FairCurvePool address to generate injectRevenue() commands
  --sandbox               Allow unfunded dev PayWord chains (for local testing only)
  --free-paths <csv>      Unmetered paths (default: /health,/.well-known/x402)

Live operator endpoints:
  GET /_paywall/status          Live earnings & verified calls JSON
  GET /_paywall/claim-commands  Ready-to-run Foundry cast claim() + injectRevenue() commands
`);
    process.exit(opts.help ? 0 : 1);
  }
  const { server, chain } = createPaywallServer(opts);
  server.listen(opts.port, opts.host, () => {
    console.log(
      `[newsatoshi-paywall] listening on http://${opts.host}:${opts.port} -> ${opts.upstream}\n` +
      `  chain:    ${chain.label} (${chain.caip2}) · contract ${chain.contract}\n` +
      `  payTo:    ${opts.payTo} · price: $${Number(opts.priceUsdc).toFixed(6)} USDC/call (0% fee)\n` +
      `  mode:     ${opts.sandbox ? "SANDBOX (dev)" : "PRODUCTION (live on-chain channel funding check)"}\n` +
      `  status:   http://127.0.0.1:${opts.port}/_paywall/status\n` +
      `  settle:   http://127.0.0.1:${opts.port}/_paywall/claim-commands`
    );
  });
}
