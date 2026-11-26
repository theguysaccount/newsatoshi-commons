/**
 * @newsatoshi/402 — Zero-Dependency Post-Quantum HTTP 402 PayWord & MCP SDK
 *
 * Works in Node 18+, Cloudflare Workers, Vercel Edge, Deno, Bun, and modern browsers.
 * Uses pure WebCrypto SHA-256 (post-quantum, zero gas per call, sub-millisecond verify).
 *
 * Zero fake data: all hashes, chains, and network queries are cryptographically real.
 */

export const DEFAULT_COMMONS_URL = "https://commons.newsatoshi.org";
export const MAX_PAYWORD_TICKS = 100_000;

function bytesToHex(bytes) {
  let out = "";
  for (let i = 0; i < bytes.length; i++) {
    out += bytes[i].toString(16).padStart(2, "0");
  }
  return out;
}

function hexToBytes(hex) {
  if (typeof hex !== "string") throw new Error("expected hex string");
  const clean = hex.trim().toLowerCase().replace(/^0x/, "");
  if (clean.length !== 64 || !/^[0-9a-f]{64}$/.test(clean)) {
    throw new Error("expected 64-character lowercase hex digest (32 bytes)");
  }
  const out = new Uint8Array(32);
  for (let i = 0; i < 32; i++) {
    out[i] = parseInt(clean.slice(i * 2, i * 2 + 2), 16);
  }
  return out;
}

function constantTimeEqual(a, b) {
  if (a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i++) {
    diff |= a[i] ^ b[i];
  }
  return diff === 0;
}

async function sha256(bytes) {
  const digest = await globalThis.crypto.subtle.digest("SHA-256", bytes);
  return new Uint8Array(digest);
}

/**
 * Generates a fresh CSPRNG PayWord SHA-256 hash-chain for streaming HTTP 402 payments.
 *
 * `chain[0]` is the secret seed; `chain[maxTicks]` is the public `tip`.
 * For cumulative call `#k` (`1 <= k <= maxTicks`), reveal `token = chain[maxTicks - k]`.
 */
export async function generatePayWordChain(maxTicks, unitPriceBaseUnits = 1_000) {
  if (!Number.isInteger(maxTicks) || maxTicks < 1 || maxTicks > MAX_PAYWORD_TICKS) {
    throw new Error(`maxTicks must be an integer in 1..=${MAX_PAYWORD_TICKS}`);
  }
  if (!Number.isInteger(unitPriceBaseUnits) || unitPriceBaseUnits < 1) {
    throw new Error("unitPriceBaseUnits must be a positive integer");
  }

  const seed = new Uint8Array(32);
  globalThis.crypto.getRandomValues(seed);

  const chain = new Array(maxTicks + 1);
  chain[0] = seed;
  let cur = seed;
  for (let i = 1; i <= maxTicks; i++) {
    cur = await sha256(cur);
    chain[i] = cur;
  }

  const tipHex = bytesToHex(chain[maxTicks]);
  let spentTicks = 0;

  return {
    tipHex,
    maxTicks,
    unitPriceBaseUnits,
    get spentTicks() {
      return spentTicks;
    },
    get remainingTicks() {
      return maxTicks - spentTicks;
    },
    /** Returns the next cumulative PayWord header value (`X-Newsatoshi-PayWord`). */
    nextPayment(ticksIncrement = 1) {
      if (!Number.isInteger(ticksIncrement) || ticksIncrement < 1) {
        throw new Error("ticksIncrement must be >= 1");
      }
      if (spentTicks + ticksIncrement > maxTicks) {
        throw new Error("PayWord channel exhausted — open a new channel");
      }
      spentTicks += ticksIncrement;
      const tokenHex = bytesToHex(chain[maxTicks - spentTicks]);
      return {
        tipHex,
        tokenHex,
        ticks: spentTicks,
        totalPaidBaseUnits: spentTicks * unitPriceBaseUnits,
        headerValue: `tip=${tipHex},token=${tokenHex},ticks=${spentTicks}`,
      };
    },
  };
}

/**
 * Verifies a post-quantum PayWord micropayment token locally (`SHA256^ticks(token) == tip`).
 */
export async function verifyPayWordToken(tipHex, tokenHex, ticks, unitPriceBaseUnits = 1_000) {
  if (!Number.isInteger(ticks) || ticks < 1 || ticks > MAX_PAYWORD_TICKS) {
    return { valid: false, reason: `ticks must be in 1..=${MAX_PAYWORD_TICKS}` };
  }
  let tipBytes;
  let cur;
  try {
    tipBytes = hexToBytes(tipHex);
    cur = hexToBytes(tokenHex);
  } catch (err) {
    return { valid: false, reason: err.message };
  }
  for (let i = 0; i < ticks; i++) {
    cur = await sha256(cur);
  }
  const valid = constantTimeEqual(cur, tipBytes);
  return {
    valid,
    ticks,
    totalPaidBaseUnits: valid ? ticks * unitPriceBaseUnits : 0,
    computedTipHex: bytesToHex(cur),
    reason: valid ? null : "SHA256^ticks(token) did not match committed tip",
  };
}

/**
 * Parses `X-Newsatoshi-PayWord: tip=<64hex>,token=<64hex>,ticks=<int>`.
 */
export function parsePayWordHeader(headerValue) {
  if (typeof headerValue !== "string" || !headerValue.trim()) return null;
  const fields = {};
  for (const part of headerValue.split(",")) {
    const idx = part.indexOf("=");
    if (idx <= 0) continue;
    fields[part.slice(0, idx).trim()] = part.slice(idx + 1).trim();
  }
  if (!fields.tip || !fields.token || !fields.ticks) return null;
  const ticks = Number(fields.ticks);
  if (!Number.isInteger(ticks) || ticks < 1) return null;
  return { tipHex: fields.tip, tokenHex: fields.token, ticks };
}

/**
 * Builds a standards-compliant `NEWSATOSHI-402/1.0` Payment Required challenge payload & header.
 */
export async function build402Challenge({
  resource = "/api/inference",
  amountLove = "0.01",
  network = "mainnet",
  recipient = "<RECIPIENT_NSD1_OR_NS1_ADDRESS>",
  unitPriceBaseUnits = 1_000,
} = {}) {
  const net = network === "testnet" ? "testnet" : "mainnet";
  const ts = Math.floor(Date.now() / 1000);
  const seedBytes = new TextEncoder().encode(
    `NEWSATOSHI-402:${net}:${resource}:${amountLove}:${recipient}:${ts}`
  );
  const paymentHashHex = bytesToHex(await sha256(seedBytes));
  return {
    status: 402,
    wwwAuthenticate: `Newsatoshi-402 realm="newsatoshi", network="${net}", amount="${amountLove}"`,
    body: {
      status: 402,
      protocol: "NEWSATOSHI-402/1.0",
      resource,
      network: net,
      amount_love: String(amountLove),
      payment_hash_hex: paymentHashHex,
      issued_at_unix: ts,
      rails: {
        l1_hash_lock: {
          method: "newsatoshi_lock",
          cli_command: `newsatoshi --net ${net} lock ${amountLove} --hash ${paymentHashHex} --to ${recipient} --blocks 60`,
          header_format: `X-Newsatoshi-Payment: rail=l1_lock,network=${net},hash=${paymentHashHex},preimage=<64hex>`,
        },
        payword_stream: {
          method: "sha256_hash_chain",
          unit_price_base_units: unitPriceBaseUnits,
          invariant: "SHA256^ticks(token) == tip",
          header_format: "X-Newsatoshi-PayWord: tip=<64hex>,token=<64hex>,ticks=<u64>",
        },
      },
    },
  };
}

/**
 * Drop-in HTTP 402 PayWord payment gate with monotonic per-tip replay protection.
 * Works with Express, Fastify, Node http.createServer, Next.js App Router, and Workers.
 */
export function createPayWordGate({
  minTicksIncrement = 1,
  unitPriceBaseUnits = 1_000,
  amountLove = "0.01",
  network = "mainnet",
  recipient = "<RECIPIENT_NSD1_OR_NS1_ADDRESS>",
  allowedTips = null,
} = {}) {
  const highWaterByTip = new Map();

  return {
    highWaterByTip,
    /**
     * Verifies an incoming `X-Newsatoshi-PayWord` header, enforcing strictly increasing ticks
     * per committed channel `tip` (blocks token replay).
     */
    async verifyRequest(headerValue, resource = "/api/resource") {
      const parsed = parsePayWordHeader(headerValue);
      if (!parsed) {
        return {
          paid: false,
          challenge: await build402Challenge({
            resource,
            amountLove,
            network,
            recipient,
            unitPriceBaseUnits,
          }),
        };
      }
      const normTip = parsed.tipHex.trim().toLowerCase();
      if (allowedTips && !allowedTips.has(normTip)) {
        return {
          paid: false,
          reason: "unrecognized PayWord channel tip",
          challenge: await build402Challenge({
            resource,
            amountLove,
            network,
            recipient,
            unitPriceBaseUnits,
          }),
        };
      }
      const prevTicks = highWaterByTip.get(normTip) || 0;
      if (parsed.ticks < prevTicks + minTicksIncrement) {
        return {
          paid: false,
          reason: `replay or insufficient tick increment (got ${parsed.ticks}, need >= ${prevTicks + minTicksIncrement})`,
          challenge: await build402Challenge({
            resource,
            amountLove,
            network,
            recipient,
            unitPriceBaseUnits,
          }),
        };
      }
      const check = await verifyPayWordToken(
        normTip,
        parsed.tokenHex,
        parsed.ticks,
        unitPriceBaseUnits
      );
      if (!check.valid) {
        return {
          paid: false,
          reason: check.reason,
          challenge: await build402Challenge({
            resource,
            amountLove,
            network,
            recipient,
            unitPriceBaseUnits,
          }),
        };
      }
      const deltaTicks = parsed.ticks - prevTicks;
      highWaterByTip.set(normTip, parsed.ticks);
      return {
        paid: true,
        tipHex: normTip,
        ticks: parsed.ticks,
        deltaTicks,
        deltaPaidBaseUnits: deltaTicks * unitPriceBaseUnits,
        totalPaidBaseUnits: check.totalPaidBaseUnits,
      };
    },
  };
}

/**
 * Autonomous agent HTTP fetch wrapper: automatically handles HTTP 402 challenges
 * by revealing the next PayWord SHA-256 token and retrying the request once.
 */
export async function fetchWithNewsatoshi402(url, paywordWallet, fetchOptions = {}, ticksPerCall = 1) {
  const first = await fetch(url, fetchOptions);
  if (first.status !== 402) return first;

  const payment = paywordWallet.nextPayment(ticksPerCall);
  const headers = new Headers(fetchOptions.headers || {});
  headers.set("X-Newsatoshi-PayWord", payment.headerValue);
  return fetch(url, { ...fetchOptions, headers });
}

/**
 * Minimal zero-dependency Streamable HTTP MCP client for Newsatoshi Commons.
 */
export class NewsatoshiMcpClient {
  constructor(endpoint = `${DEFAULT_COMMONS_URL}/mcp`) {
    this.endpoint = endpoint;
    this._seq = 0;
  }

  async rpc(method, params = {}) {
    const id = ++this._seq;
    const resp = await fetch(this.endpoint, {
      method: "POST",
      headers: { "Content-Type": "application/json", Accept: "application/json" },
      body: JSON.stringify({ jsonrpc: "2.0", id, method, params }),
    });
    const data = await resp.json();
    if (data.error) {
      throw new Error(`MCP error (${data.error.code}): ${data.error.message}`);
    }
    return data.result;
  }

  listTools() {
    return this.rpc("tools/list");
  }

  callTool(name, args = {}) {
    return this.rpc("tools/call", { name, arguments: args });
  }
}

export const CANONICAL_EVM_HTLC = "0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987";
export const CANONICAL_BACKING_VAULT = "0x37Ed94525DBE408fA72cDd156409875666679265";

/**
 * Dual-rail Web4 HTTP 402 gate: accepts zero-gas Newsatoshi SHA-256 PayWords
 * AND advertises x402 Base USDC + NewsatoshiBackingVault revenue-backing metadata
 * so both Newsatoshi agents and Conway Automaton / Coinbase x402 agents can pay seamlessly.
 */
export function createDual402Gate({
  recipient,
  evmPayTo,
  charterId = "",
  backingBps = 2000,
  unitPriceBaseUnits = 1000,
  amountLove = "0.001",
  x402UsdcMicro = 1000,
  network = "mainnet",
  allowedTips = null,
} = {}) {
  const inner = createPayWordGate({
    recipient,
    unitPriceBaseUnits,
    amountLove,
    network,
    allowedTips,
  });

  return {
    ...inner,
    async verifyRequest(headerValue, resource = "/api/resource") {
      const res = await inner.verifyRequest(headerValue, resource);
      if (res.paid) {
        const backingUnits = Math.floor((res.deltaPaidBaseUnits * backingBps) / 10000);
        return {
          ...res,
          backingBps,
          backingAllocationBaseUnits: backingUnits,
          opsAllocationBaseUnits: res.deltaPaidBaseUnits - backingUnits,
          backingVault: CANONICAL_BACKING_VAULT,
        };
      }
      return {
        ...res,
        challenge: {
          ...res.challenge,
          x402: {
            scheme: "exact",
            network: "eip155:8453",
            asset: "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913",
            maxAmountRequired: String(x402UsdcMicro),
            payTo: evmPayTo || CANONICAL_BACKING_VAULT,
            htlcBridge: CANONICAL_EVM_HTLC,
            backingVault: CANONICAL_BACKING_VAULT,
            charterId,
            backingBps,
          },
        },
      };
    },
  };
}
