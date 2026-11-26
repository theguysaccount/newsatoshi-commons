/**
 * @newsatoshi/402 — Zero-Dependency Post-Quantum HTTP 402 PayWord & MCP SDK (v0.15.0)
 *
 * Implements incremental O(1) Rivest–Shamir (1996) PayWord SHA-256 hash-chain metering
 * paired with live Newsatoshi L1 RFC-0022 PayWord Channel verification (`/api/channel`,
 * canonical SHA-256 Merkle inclusion proof check, and 70-byte unilateral `ChannelClaim`
 * transaction broadcast via `POST /api/tx`), legacy L1 HTLC lock checks (`/api/lock`),
 * and zero-API-key Streamable HTTP MCP access for https://commons.newsatoshi.org/mcp.
 *
 * Works in Node 18+ (fast native node:crypto SHA-256), Cloudflare Workers, Vercel Edge,
 * Deno, Bun, and modern browsers (automatic WebCrypto fallback).
 *
 * Security & Honesty Guarantees:
 * - O(1) per sequential voucher: verifies SHA256^(ticks - lastTicks)(token) == lastToken
 *   (1 hash per sequential call regardless of whether ticks is 1 or 1,000,000).
 * - Supports up to 1,000,000 ticks per channel (matching L1 RFC-0022 consensus limits).
 * - Separates local hash-chain verification from on-chain solvency: production gates
 *   require a verified funded on-chain RFC-0022 channel (`fundingStatus: "verified_onchain_channel"`)
 *   or L1 lock (`fundingStatus: "verified_onchain_lock"`).
 *   Unfunded sandbox usage must explicitly opt into `allowUnfundedDemo: true` and is
 *   always labeled `fundingStatus: "unfunded_demo"` with `collectibleOnChain: false`.
 */

export const DEFAULT_COMMONS_URL = "https://commons.newsatoshi.org";
export const DEFAULT_MAINNET_SEED_URL = "https://newsatoshi.org/mainnet";
export const DEFAULT_TESTNET_SEED_URL = "https://newsatoshi-seed.fly.dev";
export const MAX_PAYWORD_TICKS = 1_000_000;
export const MAX_PAYWORD_STEP_JUMP = 100_000;

let _nodeCrypto = null;
let _nodeCryptoLoaded = false;
async function getNodeCrypto() {
  if (_nodeCryptoLoaded) return _nodeCrypto;
  _nodeCryptoLoaded = true;
  try {
    if (typeof process !== "undefined" && process.versions?.node) {
      _nodeCrypto = await import("node:crypto");
    }
  } catch {
    _nodeCrypto = null;
  }
  return _nodeCrypto;
}

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

async function sha256Iterations(inputBytes, count) {
  const nc = await getNodeCrypto();
  if (nc) {
    let cur = Buffer.from(inputBytes);
    for (let i = 0; i < count; i++) {
      cur = nc.createHash("sha256").update(cur).digest();
    }
    return new Uint8Array(cur.buffer, cur.byteOffset, cur.byteLength);
  }
  let cur = inputBytes;
  for (let i = 0; i < count; i++) {
    cur = new Uint8Array(await globalThis.crypto.subtle.digest("SHA-256", cur));
  }
  return cur;
}

async function sha256(bytes) {
  return sha256Iterations(bytes, 1);
}

/**
 * Generates a fresh CSPRNG Rivest–Shamir (1996) PayWord SHA-256 hash-chain
 * supporting up to 1,000,000 ticks per channel.
 *
 * `chain[0]` is the secret seed; `chain[maxTicks]` is the public `tip`.
 * For cumulative call `#k` (`1 <= k <= maxTicks`), reveal `token = chain[maxTicks - k]`.
 *
 * Memory optimization: for large chains (> 10,000 ticks), stores sparse checkpoints
 * every `checkpointStride` links (default 1,024) so a 1,000,000-tick wallet uses
 * ~32 KB of RAM instead of 32 MB while deriving each voucher in microseconds.
 */
export async function generatePayWordChain(
  maxTicks,
  unitPriceBaseUnits = 1_000,
  { checkpointStride = 1024 } = {}
) {
  if (!Number.isInteger(maxTicks) || maxTicks < 1 || maxTicks > MAX_PAYWORD_TICKS) {
    throw new Error(`maxTicks must be an integer in 1..=${MAX_PAYWORD_TICKS}`);
  }
  if (!Number.isInteger(unitPriceBaseUnits) || unitPriceBaseUnits < 1) {
    throw new Error("unitPriceBaseUnits must be a positive integer");
  }

  const seed = new Uint8Array(32);
  globalThis.crypto.getRandomValues(seed);

  const nc = await getNodeCrypto();
  const useDense = maxTicks <= 10_000;
  const denseChain = useDense ? new Array(maxTicks + 1) : null;
  const checkpoints = useDense ? null : new Map();

  let cur = nc ? Buffer.from(seed) : seed;
  if (useDense) {
    denseChain[0] = new Uint8Array(seed);
  } else {
    checkpoints.set(0, new Uint8Array(seed));
  }

  for (let i = 1; i <= maxTicks; i++) {
    if (nc) {
      cur = nc.createHash("sha256").update(cur).digest();
    } else {
      cur = new Uint8Array(await globalThis.crypto.subtle.digest("SHA-256", cur));
    }
    if (useDense) {
      denseChain[i] = new Uint8Array(cur.buffer, cur.byteOffset, cur.byteLength).slice();
    } else if (i % checkpointStride === 0 || i === maxTicks) {
      checkpoints.set(i, new Uint8Array(cur.buffer, cur.byteOffset, cur.byteLength).slice());
    }
  }

  const tipBytes = useDense ? denseChain[maxTicks] : checkpoints.get(maxTicks);
  const tipHex = bytesToHex(tipBytes);
  let spentTicks = 0;

  function deriveLinkSync(index) {
    if (useDense) return denseChain[index];
    const baseIdx = Math.floor(index / checkpointStride) * checkpointStride;
    let node = Buffer.from(checkpoints.get(baseIdx));
    const steps = index - baseIdx;
    for (let s = 0; s < steps; s++) {
      node = nc.createHash("sha256").update(node).digest();
    }
    return new Uint8Array(node.buffer, node.byteOffset, node.byteLength);
  }

  async function deriveLinkAsync(index) {
    if (useDense) return denseChain[index];
    if (nc) return deriveLinkSync(index);
    const baseIdx = Math.floor(index / checkpointStride) * checkpointStride;
    return sha256Iterations(checkpoints.get(baseIdx), index - baseIdx);
  }

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
    /** Returns the next cumulative PayWord header value (`X-Newsatoshi-PayWord`) synchronously in Node or for chains <= 10,000 ticks. */
    nextPayment(ticksIncrement = 1) {
      if (!Number.isInteger(ticksIncrement) || ticksIncrement < 1) {
        throw new Error("ticksIncrement must be >= 1");
      }
      if (spentTicks + ticksIncrement > maxTicks) {
        throw new Error("PayWord channel exhausted — open a new channel");
      }
      if (!useDense && !nc) {
        throw new Error("Use await nextPaymentAsync() for >10,000-tick chains in non-Node runtimes");
      }
      spentTicks += ticksIncrement;
      const tokenHex = bytesToHex(deriveLinkSync(maxTicks - spentTicks));
      return {
        tipHex,
        tokenHex,
        ticks: spentTicks,
        totalPaidBaseUnits: spentTicks * unitPriceBaseUnits,
        headerValue: `tip=${tipHex},token=${tokenHex},ticks=${spentTicks}`,
      };
    },
    /** Runtime-agnostic async version supporting up to 1,000,000 ticks in both Node and WebCrypto runtimes. */
    async nextPaymentAsync(ticksIncrement = 1) {
      if (!Number.isInteger(ticksIncrement) || ticksIncrement < 1) {
        throw new Error("ticksIncrement must be >= 1");
      }
      if (spentTicks + ticksIncrement > maxTicks) {
        throw new Error("PayWord channel exhausted — open a new channel");
      }
      spentTicks += ticksIncrement;
      const tokenBytes = await deriveLinkAsync(maxTicks - spentTicks);
      const tokenHex = bytesToHex(tokenBytes);
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
 * Verifies a post-quantum PayWord voucher locally:
 * - Incremental O(delta) mode (recommended): when `previousState = { tokenHex, ticks }`
 *   is provided, verifies `SHA256^(ticks - previousState.ticks)(token) == previousState.tokenHex`.
 *   For consecutive 1-unit purchases this executes exactly 1 SHA-256 hash (O(1)) even at tick #1,000,000.
 * - Full-chain mode (initial voucher or stateless check): verifies `SHA256^ticks(token) == tip`.
 */
export async function verifyPayWordToken(
  tipHex,
  tokenHex,
  ticks,
  unitPriceBaseUnits = 1_000,
  previousState = null
) {
  if (!Number.isInteger(ticks) || ticks < 1 || ticks > MAX_PAYWORD_TICKS) {
    return { valid: false, reason: `ticks must be in 1..=${MAX_PAYWORD_TICKS}` };
  }

  let anchorBytes;
  let cur;
  let iterations;
  let mode = "full";

  try {
    // Always validate tipHex format even in incremental mode.
    const tipBytes = hexToBytes(tipHex);
    cur = hexToBytes(tokenHex);

    if (previousState && typeof previousState.tokenHex === "string" && Number.isInteger(previousState.ticks) && previousState.ticks > 0) {
      if (ticks <= previousState.ticks) {
        return {
          valid: false,
          reason: `ticks (${ticks}) must exceed last accepted ticks (${previousState.ticks})`,
        };
      }
      iterations = ticks - previousState.ticks;
      anchorBytes = hexToBytes(previousState.tokenHex);
      mode = "incremental";
    } else {
      iterations = ticks;
      anchorBytes = tipBytes;
    }
  } catch (err) {
    return { valid: false, reason: err.message };
  }

  if (iterations > MAX_PAYWORD_STEP_JUMP) {
    return {
      valid: false,
      reason: `single-call tick jump (${iterations}) exceeds anti-DoS cap (${MAX_PAYWORD_STEP_JUMP})`,
    };
  }

  const computed = await sha256Iterations(cur, iterations);
  const valid = constantTimeEqual(computed, anchorBytes);
  return {
    valid,
    mode,
    hashesEvaluated: iterations,
    ticks,
    totalPaidBaseUnits: valid ? ticks * unitPriceBaseUnits : 0,
    computedAnchorHex: bytesToHex(computed),
    computedTipHex: mode === "full" ? bytesToHex(computed) : tipHex.trim().toLowerCase(),
    reason: valid
      ? null
      : mode === "incremental"
        ? "SHA256^(ticks - lastTicks)(token) did not match last accepted token"
        : "SHA256^ticks(token) did not match committed tip",
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
 * Verifies a canonical Newsatoshi L1 `MerkleMap` inclusion proof (`GET /api/channel?id=`)
 * against its committed root using pure SHA-256 (`nsMKL` / `nsMKB` domain tags).
 */
export async function verifyChannelMerkleProof(proof) {
  if (!proof || typeof proof !== "object") return false;
  try {
    const rootBytes = hexToBytes(proof.root);
    if (rootBytes.every((b) => b === 0)) return false;
    const keyBytes = hexToBytes(proof.key);
    const valBytes = hexToBytes(proof.value_hash ?? proof.valueHash);
    const siblings = Array.isArray(proof.siblings) ? proof.siblings : null;
    if (!siblings || siblings.length > 256) return false;

    const leafBuf = new Uint8Array(5 + 32 + 32);
    leafBuf.set([0x6e, 0x73, 0x4d, 0x4b, 0x4c], 0); // "nsMKL"
    leafBuf.set(keyBytes, 5);
    leafBuf.set(valBytes, 37);
    let cur = await sha256(leafBuf);

    const branchBuf = new Uint8Array(5 + 32 + 32);
    branchBuf.set([0x6e, 0x73, 0x4d, 0x4b, 0x42], 0); // "nsMKB"
    for (let depth = siblings.length - 1; depth >= 0; depth--) {
      const sib = hexToBytes(siblings[depth]);
      const bit = ((keyBytes[depth >> 3] >> (7 - (depth & 7))) & 1) === 1;
      if (bit) {
        branchBuf.set(sib, 5);
        branchBuf.set(cur, 37);
      } else {
        branchBuf.set(cur, 5);
        branchBuf.set(sib, 37);
      }
      cur = await sha256(branchBuf);
    }
    return constantTimeEqual(cur, rootBytes);
  } catch {
    return false;
  }
}

function hexToVariableBytes(hex, maxBytes = 8192) {
  if (typeof hex !== "string") throw new Error("expected hex string");
  const clean = hex.trim().toLowerCase().replace(/^0x/, "");
  if (clean.length % 2 !== 0 || !/^[0-9a-f]*$/.test(clean)) {
    throw new Error("expected even-length lowercase hex string");
  }
  const n = clean.length / 2;
  if (n > maxBytes) throw new Error(`hex payload (${n} bytes) exceeds limit ${maxBytes}`);
  const out = new Uint8Array(n);
  for (let i = 0; i < n; i++) {
    out[i] = parseInt(clean.slice(i * 2, i * 2 + 2), 16);
  }
  return out;
}

/**
 * Computes the exact 32-byte domain-separated message signed by the seller's FIPS 205
 * `SLH-DSA-SHA2-128s` key (`seller_vk`) to authorize an RFC-0022 `Action::ChannelClaim`:
 * `SHA256("nsCHANCLAIM" || channel_id || ticks_u32_le || token)`.
 */
export async function channelClaimMessageHex(channelIdHex, ticks, tokenHex) {
  if (!Number.isInteger(ticks) || ticks < 1 || ticks > MAX_PAYWORD_TICKS) {
    throw new Error(`ticks must be an integer in 1..=${MAX_PAYWORD_TICKS}`);
  }
  const chBytes = hexToBytes(channelIdHex);
  const tokBytes = hexToBytes(tokenHex);
  const msg = new Uint8Array(11 + 32 + 4 + 32);
  msg.set([0x6e, 0x73, 0x43, 0x48, 0x41, 0x4e, 0x43, 0x4c, 0x41, 0x49, 0x4d], 0); // "nsCHANCLAIM"
  msg.set(chBytes, 11);
  new DataView(msg.buffer).setUint32(43, ticks, true);
  msg.set(tokBytes, 47);
  return bytesToHex(await sha256(msg));
}

/**
 * Encodes an exact seller-signed Newsatoshi L1 RFC-0022 unilateral `Action::ChannelClaim`
 * transaction (`Tx::Act(11)`) ready for `POST /api/tx` on any independent node.
 * Wire layout: `0x02 || 0x0b || channel[32] || ticks_u32_le[4] || token[32] || sig_len_u32_le[4] || sig`.
 * With a standard 7,856-byte NIST FIPS 205 `SLH-DSA-SHA2-128s` signature, total size is 7,930 bytes.
 */
export function encodeChannelClaimTx(channelIdHex, ticks, tokenHex, sellerSigHex) {
  if (!Number.isInteger(ticks) || ticks < 1 || ticks > MAX_PAYWORD_TICKS) {
    throw new Error(`ticks must be an integer in 1..=${MAX_PAYWORD_TICKS}`);
  }
  const chBytes = hexToBytes(channelIdHex);
  const tokBytes = hexToBytes(tokenHex);
  const sigBytes = hexToVariableBytes(sellerSigHex ?? "", 7856);
  const out = new Uint8Array(74 + sigBytes.length);
  out[0] = 2; // Tx::Act
  out[1] = 11; // Action::ChannelClaim
  out.set(chBytes, 2);
  const dv = new DataView(out.buffer);
  dv.setUint32(34, ticks, true);
  out.set(tokBytes, 38);
  dv.setUint32(70, sigBytes.length, true);
  out.set(sigBytes, 74);
  return out;
}

/**
 * Encodes an exact 34-byte Newsatoshi L1 RFC-0022 `Action::ChannelExpire`
 * transaction (`Tx::Act(12)`) ready for `POST /api/tx` after channel timeout.
 */
export function encodeChannelExpireTx(channelIdHex) {
  const chBytes = hexToBytes(channelIdHex);
  const out = new Uint8Array(34);
  out[0] = 2; // Tx::Act
  out[1] = 12; // Action::ChannelExpire
  out.set(chBytes, 2);
  return out;
}

/**
 * Broadcasts a seller-signed unilateral RFC-0022 `ChannelClaim` transaction directly to any
 * Newsatoshi L1 node (`POST /api/tx`) with zero buyer cooperation or central gateway.
 */
export async function submitL1ChannelClaim({
  channelIdHex,
  ticks,
  tokenHex,
  sellerSigHex,
  network = "mainnet",
  seedBaseUrl = null,
  fetchImpl = fetch,
}) {
  const net = network === "testnet" ? "testnet" : "mainnet";
  const base = (
    seedBaseUrl ||
    (net === "testnet" ? DEFAULT_TESTNET_SEED_URL : DEFAULT_MAINNET_SEED_URL)
  ).replace(/\/$/, "");
  const txBytes = encodeChannelClaimTx(channelIdHex, ticks, tokenHex, sellerSigHex);
  const resp = await fetchImpl(`${base}/api/tx`, {
    method: "POST",
    headers: { "Content-Type": "application/octet-stream" },
    body: txBytes,
  });
  const data = await resp.json().catch(() => ({ ok: false, error: `HTTP ${resp.status}` }));
  return {
    ok: Boolean(resp.ok && data?.ok),
    status: resp.status,
    txid: data?.txid || null,
    txBytes: txBytes.length,
    error: data?.error || null,
  };
}

/**
 * Queries a live Newsatoshi L1 node (`GET /api/channel?tip=<hex>` or `?id=<hex>`) to
 * verify whether an RFC-0022 PayWord channel is open on-chain, funded for `sellerAddress`,
 * covers `requiredTicks` at `unitPriceBaseUnits` with sufficient `minBlocksRemaining`
 * before timeout, and passes canonical SHA-256 Merkle inclusion proof verification.
 */
export async function verifyL1ChannelFunding({
  tipHex = null,
  channelId = null,
  network = "mainnet",
  sellerAddress = null,
  sellerVk = null,
  merchantAddress = null,
  requiredTicks = 1,
  unitPriceBaseUnits = 1,
  minBlocksRemaining = 6,
  requireConfirmed = true,
  seedBaseUrl = null,
  fetchImpl = fetch,
}) {
  const net = network === "testnet" ? "testnet" : "mainnet";
  const base = (
    seedBaseUrl ||
    (net === "testnet" ? DEFAULT_TESTNET_SEED_URL : DEFAULT_MAINNET_SEED_URL)
  ).replace(/\/$/, "");
  const expectedSeller = sellerAddress || merchantAddress;

  try {
    let url;
    if (channelId) {
      const cleanId = channelId.trim().toLowerCase().replace(/^0x/, "");
      url = `${base}/api/channel?id=${cleanId}`;
    } else if (tipHex) {
      const cleanTip = tipHex.trim().toLowerCase().replace(/^0x/, "");
      url = `${base}/api/channel?tip=${cleanTip}`;
    } else {
      return { funded: false, network: net, reason: "tipHex or channelId is required" };
    }

    const resp = await fetchImpl(url);
    if (!resp.ok) {
      return {
        funded: false,
        network: net,
        reason: `L1 channel lookup returned HTTP ${resp.status}`,
      };
    }
    const data = await resp.json();
    const currentHeight = Number(data.height || 0);
    const candidates = Array.isArray(data.channels)
      ? data.channels
      : data.found
        ? [data]
        : [];
    const openChannels = candidates.filter((c) => c && c.found !== false && c.state === "open");

    if (openChannels.length === 0) {
      return {
        funded: false,
        network: net,
        currentHeight,
        reason: `no open funded RFC-0022 L1 channel found on ${net}`,
      };
    }

    const matching = openChannels.find((ch) => {
      const chHeight = Number(ch.height || currentHeight || 0);
      const maxTicks = Number(ch.max_ticks ?? 0);
      const unitPrice = Number(ch.unit_price ?? 0);
      const timeout = Number(ch.timeout ?? 0);
      const seller = String(ch.seller ?? "").trim().toLowerCase();
      const chVk = String(ch.seller_vk ?? "").trim().toLowerCase();
      const confirmedOk = !requireConfirmed || ch.confirmed === true;
      const addrOk =
        !expectedSeller ||
        expectedSeller.startsWith("<") ||
        seller === expectedSeller.trim().toLowerCase();
      const vkOk =
        !sellerVk || chVk === sellerVk.trim().toLowerCase().replace(/^0x/, "");
      const ticksOk = maxTicks >= requiredTicks;
      const priceOk = unitPrice >= unitPriceBaseUnits;
      const timeoutOk = chHeight === 0 || timeout >= chHeight + minBlocksRemaining;
      return confirmedOk && addrOk && vkOk && ticksOk && priceOk && timeoutOk;
    });

    if (!matching) {
      return {
        funded: false,
        network: net,
        currentHeight,
        reason: `open RFC-0022 channel exists on ${net} but fails confirmation, seller address, max_ticks (>= ${requiredTicks}), unit_price (>= ${unitPriceBaseUnits}), or timeout (>= +${minBlocksRemaining} blocks) check`,
      };
    }

    let merkleProofVerified = false;
    if (matching.merkle_proof) {
      merkleProofVerified = await verifyChannelMerkleProof(matching.merkle_proof);
      if (!merkleProofVerified) {
        return {
          funded: false,
          network: net,
          currentHeight,
          reason: "RFC-0022 channel Merkle inclusion proof failed local SHA-256 verification",
        };
      }
    }

    const budget = Number(matching.budget ?? Number(matching.max_ticks) * Number(matching.unit_price));
    return {
      funded: true,
      rail: "rfc0022_payword_channel",
      network: net,
      channelId: matching.id || null,
      tipHex: matching.tip || tipHex,
      maxTicks: Number(matching.max_ticks || 0),
      unitPriceBaseUnits: Number(matching.unit_price || 0),
      claimFeeBaseUnits: Number(matching.claim_fee || 0),
      budgetBaseUnits: budget,
      totalDepositBaseUnits: Number(matching.total_deposit || budget),
      amountBaseUnits: budget,
      timeoutHeight: Number(matching.timeout || 0),
      currentHeight: Number(matching.height || currentHeight || 0),
      confirmations: Number(matching.confirmations || 0),
      confirmed: Boolean(matching.confirmed),
      sellerAddress: matching.seller || null,
      sellerVk: matching.seller_vk || null,
      refundAddress: matching.refund || null,
      merkleProofVerified,
    };
  } catch (err) {
    return {
      funded: false,
      network: net,
      reason: `L1 channel verification failed: ${err.message}`,
    };
  }
}

/**
 * Queries a live Newsatoshi L1 node (`/api/lock?hash=<tip>`) to verify whether a PayWord
 * channel commitment is actually backed by an open, unspent on-chain lock paying the merchant
 * with sufficient balance and timeout safety margin.
 */
export async function verifyL1LockFunding({
  tipHex,
  network = "mainnet",
  merchantAddress = null,
  requiredBaseUnits = 1,
  minBlocksRemaining = 6,
  seedBaseUrl = null,
  fetchImpl = fetch,
}) {
  const net = network === "testnet" ? "testnet" : "mainnet";
  const base = (
    seedBaseUrl ||
    (net === "testnet" ? DEFAULT_TESTNET_SEED_URL : DEFAULT_MAINNET_SEED_URL)
  ).replace(/\/$/, "");
  const cleanTip = tipHex.trim().toLowerCase().replace(/^0x/, "");

  try {
    const [lockResp, statusResp] = await Promise.all([
      fetchImpl(`${base}/api/lock?hash=${cleanTip}`),
      fetchImpl(`${base}/status`),
    ]);
    if (!lockResp.ok) {
      return {
        funded: false,
        reason: `L1 lock lookup returned HTTP ${lockResp.status}`,
      };
    }
    const lockData = await lockResp.json();
    const statusData = statusResp.ok ? await statusResp.json() : {};
    const currentHeight = Number(statusData.height || 0);

    const openLocks = Array.isArray(lockData.locks)
      ? lockData.locks.filter((l) => l && l.found !== false && l.state === "open")
      : Array.isArray(lockData.open)
        ? lockData.open
        : lockData.open
          ? [lockData.open]
          : lockData.lock && lockData.status === "open"
            ? [lockData.lock]
            : [];

    if (openLocks.length === 0) {
      return {
        funded: false,
        network: net,
        currentHeight,
        reason: `no open funded L1 lock found on ${net} for commitment ${cleanTip.slice(0, 16)}…`,
      };
    }

    const matching = openLocks.find((l) => {
      const amount = Number(l.amount ?? l.value ?? 0);
      const timeout = Number(l.timeout ?? 0);
      const claim = String(l.claim_addr ?? l.claim ?? l.to ?? "").trim().toLowerCase();
      const addrOk =
        !merchantAddress ||
        merchantAddress.startsWith("<") ||
        claim === merchantAddress.trim().toLowerCase();
      const amountOk = amount >= requiredBaseUnits;
      const timeoutOk = currentHeight === 0 || timeout >= currentHeight + minBlocksRemaining;
      return addrOk && amountOk && timeoutOk;
    });

    if (!matching) {
      return {
        funded: false,
        network: net,
        currentHeight,
        reason: `open lock exists on ${net} but fails merchant address, solvency (>= ${requiredBaseUnits}), or timeout (>= +${minBlocksRemaining} blocks) check`,
      };
    }

    return {
      funded: true,
      network: net,
      lockId: matching.id || null,
      amountBaseUnits: Number(matching.amount ?? matching.value ?? 0),
      timeoutHeight: Number(matching.timeout ?? 0),
      currentHeight,
      claimAddress: matching.claim_addr ?? matching.claim ?? matching.to ?? null,
    };
  } catch (err) {
    return {
      funded: false,
      network: net,
      reason: `L1 funding verification failed: ${err.message}`,
    };
  }
}

/**
 * Simple atomic JSON-file store for preserving accepted PayWord channel state
 * (`lastTokenHex`, `lastTicks`, `fundedLock`) across Node process restarts.
 */
export async function createFilePayWordStore(filePath) {
  const fs = await import("node:fs");
  const data = new Map();
  if (fs.existsSync(filePath)) {
    try {
      const parsed = JSON.parse(fs.readFileSync(filePath, "utf8"));
      for (const [k, v] of Object.entries(parsed)) {
        data.set(k, v);
      }
    } catch {
      // Start clean if file is empty or corrupt.
    }
  }
  function flush() {
    const tmp = `${filePath}.tmp`;
    fs.writeFileSync(tmp, JSON.stringify(Object.fromEntries(data), null, 2));
    fs.renameSync(tmp, filePath);
  }
  return {
    get(tip) {
      return data.get(tip);
    },
    set(tip, value) {
      data.set(tip, value);
      flush();
    },
  };
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
          method: "sha256_hash_chain_rivest_shamir_1996",
          unit_price_base_units: unitPriceBaseUnits,
          max_ticks: MAX_PAYWORD_TICKS,
          verification_complexity: "O(delta) incremental (1 SHA-256 hash per sequential call)",
          invariant: "SHA256^(ticks - lastTicks)(token) == lastToken (anchored to funded on-chain lock)",
          header_format: "X-Newsatoshi-PayWord: tip=<64hex>,token=<64hex>,ticks=<u64>",
        },
      },
    },
  };
}

/**
 * Production-grade HTTP 402 PayWord payment gate with:
 * 1. Incremental O(1) verification against the last accepted voucher (`lastTokenHex`, `lastTicks`)
 * 2. Monotonic per-tip replay protection + pluggable durable `store` (survives restarts)
 * 3. Explicit on-chain funding verification (`fundingVerifier` or live L1 lock check):
 *    - Pass `requireFunding: true` (recommended for production) to reject unfunded channels.
 *    - Unfunded local/sandbox testing must explicitly opt in with `allowUnfundedDemo: true`
 *      and is always labeled `fundingStatus: "unfunded_demo"`, `collectibleOnChain: false`.
 */
export function createPayWordGate({
  minTicksIncrement = 1,
  unitPriceBaseUnits = 1_000,
  amountLove = "0.01",
  network = "mainnet",
  recipient = "<RECIPIENT_NSD1_OR_NS1_ADDRESS>",
  allowedTips = null,
  requireFunding = false,
  allowUnfundedDemo = !requireFunding,
  fundingRail = "auto",
  requireConfirmed = true,
  minBlocksRemaining = 6,
  seedBaseUrl = null,
  fetchImpl = fetch,
  fundingVerifier = null,
  store = null,
} = {}) {
  const memoryStore = new Map();
  const highWaterByTip = new Map();

  async function loadChannel(tip) {
    if (store) return await store.get(tip);
    return memoryStore.get(tip);
  }

  async function saveChannel(tip, state) {
    highWaterByTip.set(tip, state.ticks);
    if (store) {
      await store.set(tip, state);
    } else {
      memoryStore.set(tip, state);
    }
  }

  async function failChallenge(reason, resource) {
    return {
      paid: false,
      collectibleOnChain: false,
      fundingStatus: "rejected",
      reason,
      challenge: await build402Challenge({
        resource,
        amountLove,
        network,
        recipient,
        unitPriceBaseUnits,
      }),
    };
  }

  return {
    highWaterByTip,
    /**
     * Verifies an incoming `X-Newsatoshi-PayWord` header incrementally in O(delta) hashes,
     * enforcing strictly increasing ticks per committed channel `tip` and optional live
     * on-chain lock funding verification.
     */
    async verifyRequest(headerValue, resource = "/api/resource") {
      const parsed = parsePayWordHeader(headerValue);
      if (!parsed) {
        return failChallenge("missing or malformed X-Newsatoshi-PayWord header", resource);
      }

      const normTip = parsed.tipHex.trim().toLowerCase();
      if (allowedTips && !allowedTips.has(normTip)) {
        return failChallenge("unrecognized PayWord channel tip", resource);
      }

      const prevState = await loadChannel(normTip);
      const prevTicks = prevState ? prevState.ticks : 0;
      if (parsed.ticks < prevTicks + minTicksIncrement) {
        return failChallenge(
          `replay or insufficient tick increment (got ${parsed.ticks}, need >= ${prevTicks + minTicksIncrement})`,
          resource
        );
      }

      const requiredTotalBaseUnits = parsed.ticks * unitPriceBaseUnits;
      let fundedLock = prevState?.fundedLock || null;
      let fundingStatus = fundedLock
        ? fundedLock.rail === "rfc0022_payword_channel"
          ? "verified_onchain_channel"
          : "verified_onchain_lock"
        : "unfunded_demo";

      const needsFundingCheck =
        requireFunding ||
        !allowUnfundedDemo ||
        typeof fundingVerifier === "function";

      if (needsFundingCheck) {
        const cachedCovers =
          fundedLock &&
          fundedLock.funded === true &&
          (fundedLock.rail === "rfc0022_payword_channel"
            ? Number(fundedLock.maxTicks || 0) >= parsed.ticks &&
              Number(fundedLock.unitPriceBaseUnits || 0) >= unitPriceBaseUnits
            : Number(fundedLock.amountBaseUnits || 0) >= requiredTotalBaseUnits);

        if (!cachedCovers) {
          let checkFunding;
          if (typeof fundingVerifier === "function") {
            checkFunding = await fundingVerifier({
              tipHex: normTip,
              ticks: parsed.ticks,
              unitPriceBaseUnits,
              requiredBaseUnits: requiredTotalBaseUnits,
              network,
              merchantAddress: recipient,
              minBlocksRemaining,
            });
          } else if (fundingRail === "lock") {
            checkFunding = await verifyL1LockFunding({
              tipHex: normTip,
              network,
              merchantAddress: recipient,
              requiredBaseUnits: requiredTotalBaseUnits,
              minBlocksRemaining,
              seedBaseUrl,
              fetchImpl,
            });
          } else {
            checkFunding = await verifyL1ChannelFunding({
              tipHex: normTip,
              network,
              sellerAddress: recipient,
              requiredTicks: parsed.ticks,
              unitPriceBaseUnits,
              minBlocksRemaining,
              requireConfirmed,
              seedBaseUrl,
              fetchImpl,
            });
            if (!checkFunding.funded && fundingRail === "auto") {
              const lockCheck = await verifyL1LockFunding({
                tipHex: normTip,
                network,
                merchantAddress: recipient,
                requiredBaseUnits: requiredTotalBaseUnits,
                minBlocksRemaining,
                seedBaseUrl,
                fetchImpl,
              });
              if (lockCheck.funded) {
                checkFunding = lockCheck;
              }
            }
          }

          if (!checkFunding || !checkFunding.funded) {
            return failChallenge(
              checkFunding?.reason || "unfunded PayWord channel: no live funded on-chain channel or lock",
              resource
            );
          }
          fundedLock = checkFunding;
          fundingStatus =
            fundedLock.rail === "rfc0022_payword_channel"
              ? "verified_onchain_channel"
              : "verified_onchain_lock";
        }
      }

      // Incremental O(delta) hash check against last accepted voucher!
      const check = await verifyPayWordToken(
        normTip,
        parsed.tokenHex,
        parsed.ticks,
        unitPriceBaseUnits,
        prevState ? { tokenHex: prevState.tokenHex, ticks: prevState.ticks } : null
      );
      if (!check.valid) {
        return failChallenge(check.reason, resource);
      }

      const deltaTicks = parsed.ticks - prevTicks;
      await saveChannel(normTip, {
        ticks: parsed.ticks,
        tokenHex: parsed.tokenHex.trim().toLowerCase(),
        fundedLock,
        updatedAtUnix: Math.floor(Date.now() / 1000),
      });

      const collectible =
        fundingStatus === "verified_onchain_channel" ||
        fundingStatus === "verified_onchain_lock";

      return {
        paid: true,
        collectibleOnChain: collectible,
        unilateralClaimReady: fundingStatus === "verified_onchain_channel",
        channelId: fundedLock?.channelId || null,
        fundingStatus,
        fundedLock,
        verificationMode: check.mode,
        hashesEvaluated: check.hashesEvaluated,
        tipHex: normTip,
        tokenHex: parsed.tokenHex.trim().toLowerCase(),
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

  const payment =
    typeof paywordWallet.nextPaymentAsync === "function"
      ? await paywordWallet.nextPaymentAsync(ticksPerCall)
      : paywordWallet.nextPayment(ticksPerCall);
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

  inspectEvmContract(address, chain = "base") {
    return this.callTool("crypto_inspect_evm_contract", { address, chain });
  }

  bitcoinMempoolOracle() {
    return this.callTool("crypto_bitcoin_mempool_oracle", {});
  }

  decodeBolt11(invoice) {
    return this.callTool("crypto_decode_bolt11_invoice", { invoice });
  }

  quoteBaseUniswapV3(tokenIn, tokenOut, amount) {
    return this.callTool("crypto_quote_base_uniswap_v3", {
      token_in: tokenIn,
      token_out: tokenOut,
      amount: String(amount),
    });
  }

  verifyTxReceipt(txHash, chain = "base") {
    return this.callTool("crypto_verify_tx_receipt", { tx_hash: txHash, chain });
  }
}

export const CANONICAL_EVM_HTLC = "0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987";
export const CANONICAL_BACKING_VAULT = "0x37Ed94525DBE408fA72cDd156409875666679265";

/**
 * Dual-rail Web4 HTTP 402 gate: accepts incremental O(1) Newsatoshi SHA-256 PayWords
 * (with optional live on-chain funding verification) AND advertises x402 Base USDC
 * + NewsatoshiBackingVault revenue-backing metadata.
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
  requireFunding = false,
  allowUnfundedDemo = !requireFunding,
  fundingVerifier = null,
  store = null,
} = {}) {
  const inner = createPayWordGate({
    recipient,
    unitPriceBaseUnits,
    amountLove,
    network,
    allowedTips,
    requireFunding,
    allowUnfundedDemo,
    fundingVerifier,
    store,
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
