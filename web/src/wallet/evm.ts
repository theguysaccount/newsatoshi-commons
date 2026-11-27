// Pure TypeScript, zero-dependency EVM ABI encoding & Base Uniswap V3 / BackingVault / HTLC client.
// Every selector & calldata layout is verified byte-for-byte against Foundry `cast` on live Base Mainnet.

export type EvmChainKey = 'base' | 'arbitrum' | 'optimism' | 'polygon';

export interface EvmChainConfig {
  key: EvmChainKey;
  name: string;
  shortName: string;
  chainId: number;
  chainIdHex: string;
  nativeSymbol: string;
  nativeDecimals: number;
  rpcUrls: string[];
  explorerUrl: string;
  usdcAddress: string;
  usdcDecimals: number;
  vaultAddress: string;
  htlcAddress: string;
}

export const CANONICAL_BACKING_VAULT = '0x37Ed94525DBE408fA72cDd156409875666679265';
export const CANONICAL_HTLC = '0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987';
export const BASE_WLOVE_ADDRESS = '0x0A3ccFb3fF981a22A264Aa05CD8b77d06C2732A3';
export const BASE_WLOVE_POOL = '0x08ddc8C3fc09ACdab6802a13B26631f47ABcB888';
export const BASE_WETH_ADDRESS = '0x4200000000000000000000000000000000000006';
export const BASE_USDC_ADDRESS = '0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913';
export const BASE_UNISWAP_ROUTER = '0x2626664c2603336E57B271c5C0b26F421741e481';
export const BASE_UNISWAP_QUOTER_V2 = '0x3d4e44Eb1374240CE5F1B871ab261CD16335B76a';
export const BASE_WLOVE_FEE = 10000; // 1.00%
export const BASE_USDC_WETH_FEE = 500; // 0.05%
const ROUTER_ADDRESS_THIS_SENTINEL = '0x0000000000000000000000000000000000000002';
export const GLOBAL_COMMONS_CHARTER_ID =
  '0x0000000000000000000000000000000000000000000000000000000000000000';
export const ZERO_BYTES32 =
  '0x0000000000000000000000000000000000000000000000000000000000000000';

export const EVM_CHAINS: Record<EvmChainKey, EvmChainConfig> = {
  base: {
    key: 'base',
    name: 'Base',
    shortName: 'Base',
    chainId: 8453,
    chainIdHex: '0x2105',
    nativeSymbol: 'ETH',
    nativeDecimals: 18,
    rpcUrls: ['https://mainnet.base.org', 'https://base-rpc.publicnode.com'],
    explorerUrl: 'https://basescan.org',
    usdcAddress: BASE_USDC_ADDRESS,
    usdcDecimals: 6,
    vaultAddress: CANONICAL_BACKING_VAULT,
    htlcAddress: CANONICAL_HTLC,
  },
  arbitrum: {
    key: 'arbitrum',
    name: 'Arbitrum One',
    shortName: 'Arbitrum',
    chainId: 42161,
    chainIdHex: '0xa4b1',
    nativeSymbol: 'ETH',
    nativeDecimals: 18,
    rpcUrls: ['https://arb1.arbitrum.io/rpc', 'https://arbitrum-one-rpc.publicnode.com'],
    explorerUrl: 'https://arbiscan.io',
    usdcAddress: '0xaf88d065e77c8cC2239327C5EDb3A432268e5831',
    usdcDecimals: 6,
    vaultAddress: CANONICAL_BACKING_VAULT,
    htlcAddress: CANONICAL_HTLC,
  },
  optimism: {
    key: 'optimism',
    name: 'Optimism',
    shortName: 'Optimism',
    chainId: 10,
    chainIdHex: '0xa',
    nativeSymbol: 'ETH',
    nativeDecimals: 18,
    rpcUrls: ['https://mainnet.optimism.io', 'https://optimism-rpc.publicnode.com'],
    explorerUrl: 'https://optimistic.etherscan.io',
    usdcAddress: '0x0b2C639c533813f4Aa9D7837CAf62653d097Ff85',
    usdcDecimals: 6,
    vaultAddress: CANONICAL_BACKING_VAULT,
    htlcAddress: CANONICAL_HTLC,
  },
  polygon: {
    key: 'polygon',
    name: 'Polygon PoS',
    shortName: 'Polygon',
    chainId: 137,
    chainIdHex: '0x89',
    nativeSymbol: 'POL',
    nativeDecimals: 18,
    rpcUrls: ['https://polygon.drpc.org', 'https://polygon-bor-rpc.publicnode.com'],
    explorerUrl: 'https://polygonscan.com',
    usdcAddress: '0x3c499c542cEF5E3811e1192ce70d8cC03d5c3359',
    usdcDecimals: 6,
    vaultAddress: CANONICAL_BACKING_VAULT,
    htlcAddress: CANONICAL_HTLC,
  },
};

export function chainById(chainId: number | null): EvmChainConfig | null {
  if (!chainId) return null;
  for (const cfg of Object.values(EVM_CHAINS)) {
    if (cfg.chainId === chainId) return cfg;
  }
  return null;
}

// ── Hex & ABI Primitive Encoders ───────────────────────────────────────────────

function strip0x(hex: string): string {
  return hex.startsWith('0x') || hex.startsWith('0X') ? hex.slice(2) : hex;
}

export function padAddress(addr: string): string {
  const clean = strip0x(addr.trim()).toLowerCase();
  if (!/^[0-9a-f]{40}$/.test(clean)) {
    throw new Error(`Invalid EVM address: ${addr}`);
  }
  return clean.padStart(64, '0');
}

export function padBytes32(hex: string): string {
  const clean = strip0x(hex.trim()).toLowerCase();
  if (!/^[0-9a-f]{64}$/.test(clean)) {
    throw new Error(`Invalid bytes32 value: ${hex}`);
  }
  return clean;
}

export function padUint(value: bigint | number): string {
  const n = typeof value === 'bigint' ? value : BigInt(value);
  if (n < 0n) throw new Error('Unsigned ABI integer cannot be negative');
  return n.toString(16).padStart(64, '0');
}

function encodeDynamicBytes(rawHex: string): string {
  const clean = strip0x(rawHex).toLowerCase();
  const byteLen = clean.length / 2;
  const paddedLen = Math.ceil(clean.length / 64) * 64;
  return padUint(byteLen) + clean.padEnd(paddedLen, '0');
}

export function encodeV3Path(tokens: string[], fees: number[]): string {
  if (tokens.length !== fees.length + 1) {
    throw new Error('Uniswap V3 path requires tokens.length === fees.length + 1');
  }
  let out = '';
  for (let i = 0; i < tokens.length; i += 1) {
    const t = strip0x(tokens[i]).toLowerCase();
    if (!/^[0-9a-f]{40}$/.test(t)) throw new Error(`Invalid token address in path: ${tokens[i]}`);
    out += t;
    if (i < fees.length) {
      out += fees[i].toString(16).padStart(6, '0');
    }
  }
  return out;
}

// ── Decimal Parsing & Formatting ───────────────────────────────────────────────

export function parseUnits(amountStr: string, decimals: number): bigint | null {
  const clean = amountStr.trim().replace(/,/g, '');
  if (!clean || !/^\d*(\.\d*)?$/.test(clean) || clean === '.') return null;
  const [wholeRaw, fracRaw = ''] = clean.split('.');
  const whole = wholeRaw || '0';
  const frac = fracRaw.slice(0, decimals).padEnd(decimals, '0');
  const combined = `${whole}${frac}`.replace(/^0+/, '') || '0';
  return BigInt(combined);
}

export function formatUnits(value: bigint, decimals: number, maxFrac = 6): string {
  const negative = value < 0n;
  const abs = negative ? -value : value;
  const base = 10n ** BigInt(decimals);
  const whole = abs / base;
  const fracBig = abs % base;
  if (maxFrac === 0 || fracBig === 0n) {
    return `${negative ? '-' : ''}${whole.toLocaleString('en-US')}`;
  }
  const fracPadded = fracBig.toString().padStart(decimals, '0').slice(0, maxFrac).replace(/0+$/, '');
  if (!fracPadded) {
    return `${negative ? '-' : ''}${whole.toLocaleString('en-US')}`;
  }
  return `${negative ? '-' : ''}${whole.toLocaleString('en-US')}.${fracPadded}`;
}

export function formatCompactToken(value: bigint, decimals: number): string {
  const num = Number(formatUnits(value, decimals, 6).replace(/,/g, ''));
  if (!Number.isFinite(num)) return formatUnits(value, decimals, 2);
  if (num >= 1_000_000) return `${(num / 1_000_000).toFixed(2)}M`;
  if (num >= 10_000) return `${(num / 1_000).toFixed(2)}K`;
  if (num >= 1) return num.toLocaleString('en-US', { maximumFractionDigits: 4 });
  if (num >= 0.0001) return num.toFixed(6).replace(/0+$/, '').replace(/\.$/, '');
  if (num > 0) return num.toExponential(2);
  return '0';
}

// ── ERC-20 Calldata Encoders ───────────────────────────────────────────────────

export function encodeBalanceOf(owner: string): string {
  return `0x70a08231${padAddress(owner)}`;
}

export function encodeAllowance(owner: string, spender: string): string {
  return `0xdd62ed3e${padAddress(owner)}${padAddress(spender)}`;
}

export function encodeApprove(spender: string, amount: bigint): string {
  return `0x095ea7b3${padAddress(spender)}${padUint(amount)}`;
}

// ── Backing Vault & HTLC Calldata Encoders ─────────────────────────────────────

export function encodeDepositEth(charterId = GLOBAL_COMMONS_CHARTER_ID, memoHash = ZERO_BYTES32): string {
  return `0x17c819c4${padBytes32(charterId)}${padBytes32(memoHash)}`;
}

export function encodeDepositErc20(
  token: string,
  amount: bigint,
  charterId = GLOBAL_COMMONS_CHARTER_ID,
  memoHash = ZERO_BYTES32
): string {
  return `0x094ab57c${padBytes32(charterId)}${padAddress(token)}${padUint(amount)}${padBytes32(memoHash)}`;
}

export function encodeHtlcLockEth(recipient: string, paymentHash: string, timelockUnix: number): string {
  return `0x42b1eb79${padAddress(recipient)}${padBytes32(paymentHash)}${padUint(timelockUnix)}`;
}

export function encodeHtlcLockErc20(
  recipient: string,
  token: string,
  amount: bigint,
  paymentHash: string,
  timelockUnix: number
): string {
  return `0xe3041377${padAddress(recipient)}${padAddress(token)}${padUint(amount)}${padBytes32(paymentHash)}${padUint(timelockUnix)}`;
}

// ── Uniswap V3 Base QuoterV2 & SwapRouter02 Encoders ───────────────────────────

export function encodeQuoteExactInputSingle(
  tokenIn: string,
  tokenOut: string,
  amountIn: bigint,
  fee: number
): string {
  // quoteExactInputSingle((address,address,uint256,uint24,uint160)) -> 0xc6a5026a
  return (
    '0xc6a5026a' +
    padAddress(tokenIn) +
    padAddress(tokenOut) +
    padUint(amountIn) +
    padUint(fee) +
    padUint(0n)
  );
}

export function encodeQuoteExactInput(pathHex: string, amountIn: bigint): string {
  // quoteExactInput(bytes,uint256) -> 0xcdca1753
  return '0xcdca1753' + padUint(64) + padUint(amountIn) + encodeDynamicBytes(pathHex);
}

export function encodeExactInputSingle(params: {
  tokenIn: string;
  tokenOut: string;
  fee: number;
  recipient: string;
  amountIn: bigint;
  amountOutMinimum: bigint;
}): string {
  // exactInputSingle((address,address,uint24,address,uint256,uint256,uint160)) -> 0x04e45aaf
  return (
    '0x04e45aaf' +
    padAddress(params.tokenIn) +
    padAddress(params.tokenOut) +
    padUint(params.fee) +
    padAddress(params.recipient) +
    padUint(params.amountIn) +
    padUint(params.amountOutMinimum) +
    padUint(0n)
  );
}

export function encodeExactInput(params: {
  pathHex: string;
  recipient: string;
  amountIn: bigint;
  amountOutMinimum: bigint;
}): string {
  // exactInput((bytes,address,uint256,uint256)) -> 0xb858183f
  // Tuple contains dynamic `bytes`, so outer pointer = 0x20, inner `path` pointer = 0x80 (4 words)
  return (
    '0xb858183f' +
    padUint(32) +
    padUint(128) +
    padAddress(params.recipient) +
    padUint(params.amountIn) +
    padUint(params.amountOutMinimum) +
    encodeDynamicBytes(params.pathHex)
  );
}

export function encodeUnwrapWeth9(amountMinimum: bigint, recipient: string): string {
  // unwrapWETH9(uint256,address) -> 0x49404b7c
  return '0x49404b7c' + padUint(amountMinimum) + padAddress(recipient);
}

export function encodeRouterMulticall(deadlineUnix: number, calls: string[]): string {
  // multicall(uint256,bytes[]) -> 0x5ae401dc
  const encodedElements = calls.map((c) => encodeDynamicBytes(c));
  const offsetsStartBytes = calls.length * 32;
  let runningBytes = offsetsStartBytes;
  const offsetWords: string[] = [];
  for (const el of encodedElements) {
    offsetWords.push(padUint(runningBytes));
    runningBytes += el.length / 2;
  }
  return (
    '0x5ae401dc' +
    padUint(deadlineUnix) +
    padUint(64) +
    padUint(calls.length) +
    offsetWords.join('') +
    encodedElements.join('')
  );
}

// ── Live Public JSON-RPC Helpers (with Fallback) ───────────────────────────────

export async function rpcCall(
  chainKey: EvmChainKey,
  method: string,
  params: unknown[]
): Promise<string> {
  const cfg = EVM_CHAINS[chainKey];
  let lastErr: Error | null = null;
  for (const url of cfg.rpcUrls) {
    try {
      const res = await fetch(url, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ jsonrpc: '2.0', id: 1, method, params }),
      });
      if (!res.ok) {
        lastErr = new Error(`RPC HTTP ${res.status}`);
        continue;
      }
      const json = (await res.json()) as {
        result?: string;
        error?: { message?: string; data?: string };
      };
      if (json.error) {
        throw new Error(json.error.message || 'RPC execution reverted');
      }
      if (typeof json.result !== 'string') {
        throw new Error('Malformed RPC response');
      }
      return json.result;
    } catch (err) {
      lastErr = err instanceof Error ? err : new Error(String(err));
      // If it was an actual contract revert, don't retry blindly across RPCs
      if (/revert|execution/i.test(lastErr.message)) break;
    }
  }
  throw lastErr || new Error(`Failed RPC call on ${cfg.name}`);
}

export async function readNativeBalance(chainKey: EvmChainKey, account: string): Promise<bigint> {
  const hex = await rpcCall(chainKey, 'eth_getBalance', [account, 'latest']);
  return BigInt(hex || '0x0');
}

export async function readErc20Balance(
  chainKey: EvmChainKey,
  token: string,
  account: string
): Promise<bigint> {
  const hex = await rpcCall(chainKey, 'eth_call', [
    { to: token, data: encodeBalanceOf(account) },
    'latest',
  ]);
  if (!hex || hex === '0x') return 0n;
  return BigInt(hex.slice(0, 66));
}

export async function readErc20Allowance(
  chainKey: EvmChainKey,
  token: string,
  owner: string,
  spender: string
): Promise<bigint> {
  const hex = await rpcCall(chainKey, 'eth_call', [
    { to: token, data: encodeAllowance(owner, spender) },
    'latest',
  ]);
  if (!hex || hex === '0x') return 0n;
  return BigInt(hex.slice(0, 66));
}

// ── High-Level Base Uniswap V3 Swap Quotes & TX Builders ───────────────────────

export type BaseSwapAsset = 'ETH' | 'USDC' | 'wLOVE';

export interface BaseSwapQuote {
  fromAsset: BaseSwapAsset;
  toAsset: BaseSwapAsset;
  amountInRaw: bigint;
  amountOutRaw: bigint;
  gasEstimate: bigint;
  routeLabel: string;
  poolFeeLabel: string;
  priceImpactPct: number | null;
}

export interface BasePoolSnapshot {
  wethWei: bigint;
  wloveWei: bigint;
  wlovePerEthSpot: number;
}

export async function fetchBaseWlovePoolSnapshot(): Promise<BasePoolSnapshot> {
  const [wethWei, wloveWei] = await Promise.all([
    readErc20Balance('base', BASE_WETH_ADDRESS, BASE_WLOVE_POOL),
    readErc20Balance('base', BASE_WLOVE_ADDRESS, BASE_WLOVE_POOL),
  ]);
  const wethNum = Number(wethWei) / 1e18;
  const wloveNum = Number(wloveWei) / 1e18;
  const wlovePerEthSpot = wethNum > 0 ? wloveNum / wethNum : 0;
  return { wethWei, wloveWei, wlovePerEthSpot };
}

export async function quoteBaseSwap(
  fromAsset: BaseSwapAsset,
  toAsset: BaseSwapAsset,
  amountInRaw: bigint,
  poolSnapshot?: BasePoolSnapshot | null
): Promise<BaseSwapQuote> {
  if (amountInRaw <= 0n) throw new Error('Enter an amount greater than 0');
  if (fromAsset === toAsset) throw new Error('Select two different tokens');

  let calldata = '';
  let routeLabel = '';
  let poolFeeLabel = '1.00% Uniswap V3';

  if (fromAsset === 'ETH' && toAsset === 'wLOVE') {
    calldata = encodeQuoteExactInputSingle(
      BASE_WETH_ADDRESS,
      BASE_WLOVE_ADDRESS,
      amountInRaw,
      BASE_WLOVE_FEE
    );
    routeLabel = 'ETH → wLOVE (Base Uniswap V3)';
  } else if (fromAsset === 'wLOVE' && toAsset === 'ETH') {
    calldata = encodeQuoteExactInputSingle(
      BASE_WLOVE_ADDRESS,
      BASE_WETH_ADDRESS,
      amountInRaw,
      BASE_WLOVE_FEE
    );
    routeLabel = 'wLOVE → ETH (Base Uniswap V3)';
  } else if (fromAsset === 'USDC' && toAsset === 'wLOVE') {
    const path = encodeV3Path(
      [BASE_USDC_ADDRESS, BASE_WETH_ADDRESS, BASE_WLOVE_ADDRESS],
      [BASE_USDC_WETH_FEE, BASE_WLOVE_FEE]
    );
    calldata = encodeQuoteExactInput(path, amountInRaw);
    routeLabel = 'USDC → WETH → wLOVE (2-hop Uniswap V3)';
    poolFeeLabel = '0.05% + 1.00% V3';
  } else if (fromAsset === 'wLOVE' && toAsset === 'USDC') {
    const path = encodeV3Path(
      [BASE_WLOVE_ADDRESS, BASE_WETH_ADDRESS, BASE_USDC_ADDRESS],
      [BASE_WLOVE_FEE, BASE_USDC_WETH_FEE]
    );
    calldata = encodeQuoteExactInput(path, amountInRaw);
    routeLabel = 'wLOVE → WETH → USDC (2-hop Uniswap V3)';
    poolFeeLabel = '1.00% + 0.05% V3';
  } else {
    throw new Error(`Unsupported swap pair: ${fromAsset} → ${toAsset}`);
  }

  const resHex = await rpcCall('base', 'eth_call', [
    { to: BASE_UNISWAP_QUOTER_V2, data: calldata },
    'latest',
  ]);
  const clean = strip0x(resHex);
  if (clean.length < 64) {
    throw new Error('Pool liquidity exceeded for this size — try a smaller amount');
  }
  const amountOutRaw = BigInt(`0x${clean.slice(0, 64)}`);
  // Last 32-byte word of QuoterV2 response is gasEstimate for single-hop (word index 3)
  let gasEstimate = 150_000n;
  if (fromAsset === 'ETH' || toAsset === 'ETH') {
    if (clean.length >= 256) {
      gasEstimate = BigInt(`0x${clean.slice(192, 256)}`);
    }
  } else {
    gasEstimate = 210_000n;
  }

  // Honest real-time price impact estimation for direct ETH <-> wLOVE pairs using pool spot ratio
  let priceImpactPct: number | null = null;
  if (poolSnapshot && poolSnapshot.wlovePerEthSpot > 0) {
    if (fromAsset === 'ETH' && toAsset === 'wLOVE') {
      const effectiveRate = Number(amountOutRaw) / Number(amountInRaw);
      const impact = (1 - effectiveRate / poolSnapshot.wlovePerEthSpot) * 100;
      priceImpactPct = Math.max(0, Number.isFinite(impact) ? impact : 0);
    } else if (fromAsset === 'wLOVE' && toAsset === 'ETH') {
      const effectiveRate = Number(amountInRaw) / Number(amountOutRaw);
      const impact = (1 - poolSnapshot.wlovePerEthSpot / effectiveRate) * 100;
      priceImpactPct = Math.max(0, Number.isFinite(impact) ? impact : 0);
    }
  }

  return {
    fromAsset,
    toAsset,
    amountInRaw,
    amountOutRaw,
    gasEstimate,
    routeLabel,
    poolFeeLabel,
    priceImpactPct,
  };
}

export function buildBaseSwapTransaction(params: {
  fromAsset: BaseSwapAsset;
  toAsset: BaseSwapAsset;
  amountInRaw: bigint;
  minAmountOutRaw: bigint;
  recipient: string;
  deadlineSecondsFromNow?: number;
}): { to: string; data: string; valueHex: string } {
  const deadline = Math.floor(Date.now() / 1000) + (params.deadlineSecondsFromNow ?? 600);

  if (params.fromAsset === 'ETH' && params.toAsset === 'wLOVE') {
    const swapCall = encodeExactInputSingle({
      tokenIn: BASE_WETH_ADDRESS,
      tokenOut: BASE_WLOVE_ADDRESS,
      fee: BASE_WLOVE_FEE,
      recipient: params.recipient,
      amountIn: params.amountInRaw,
      amountOutMinimum: params.minAmountOutRaw,
    });
    return {
      to: BASE_UNISWAP_ROUTER,
      data: encodeRouterMulticall(deadline, [swapCall]),
      valueHex: `0x${params.amountInRaw.toString(16)}`,
    };
  }

  if (params.fromAsset === 'wLOVE' && params.toAsset === 'ETH') {
    const swapCall = encodeExactInputSingle({
      tokenIn: BASE_WLOVE_ADDRESS,
      tokenOut: BASE_WETH_ADDRESS,
      fee: BASE_WLOVE_FEE,
      recipient: ROUTER_ADDRESS_THIS_SENTINEL,
      amountIn: params.amountInRaw,
      amountOutMinimum: params.minAmountOutRaw,
    });
    const unwrapCall = encodeUnwrapWeth9(params.minAmountOutRaw, params.recipient);
    return {
      to: BASE_UNISWAP_ROUTER,
      data: encodeRouterMulticall(deadline, [swapCall, unwrapCall]),
      valueHex: '0x0',
    };
  }

  if (params.fromAsset === 'USDC' && params.toAsset === 'wLOVE') {
    const path = encodeV3Path(
      [BASE_USDC_ADDRESS, BASE_WETH_ADDRESS, BASE_WLOVE_ADDRESS],
      [BASE_USDC_WETH_FEE, BASE_WLOVE_FEE]
    );
    const swapCall = encodeExactInput({
      pathHex: path,
      recipient: params.recipient,
      amountIn: params.amountInRaw,
      amountOutMinimum: params.minAmountOutRaw,
    });
    return {
      to: BASE_UNISWAP_ROUTER,
      data: encodeRouterMulticall(deadline, [swapCall]),
      valueHex: '0x0',
    };
  }

  if (params.fromAsset === 'wLOVE' && params.toAsset === 'USDC') {
    const path = encodeV3Path(
      [BASE_WLOVE_ADDRESS, BASE_WETH_ADDRESS, BASE_USDC_ADDRESS],
      [BASE_WLOVE_FEE, BASE_USDC_WETH_FEE]
    );
    const swapCall = encodeExactInput({
      pathHex: path,
      recipient: params.recipient,
      amountIn: params.amountInRaw,
      amountOutMinimum: params.minAmountOutRaw,
    });
    return {
      to: BASE_UNISWAP_ROUTER,
      data: encodeRouterMulticall(deadline, [swapCall]),
      valueHex: '0x0',
    };
  }

  throw new Error(`Unsupported swap route: ${params.fromAsset} → ${params.toAsset}`);
}

export async function sha256HexUtf8(text: string): Promise<string> {
  const bytes = new TextEncoder().encode(text);
  const digest = new Uint8Array(await window.crypto.subtle.digest('SHA-256', bytes));
  return (
    '0x' +
    Array.from(digest)
      .map((b) => b.toString(16).padStart(2, '0'))
      .join('')
  );
}

// ── Live Base Mainnet FairCurveFactory & PayWordChannel Encoders (v0.30.0) ────

export const BASE_PAYWORD_CHANNEL_ADDRESS = '0x971BA814Df91c20DcA3aDd870C2B55d643965C50';
export const BASE_FAIR_CURVE_FACTORY_ADDRESS = '0xF6bE782819a8eD0D1556A8f5DC3223f9eb13a933';
export const ZERO_ADDRESS = '0x0000000000000000000000000000000000000000';

function encodeAbiStringWord(str: string): { hex: string; byteLen: number } {
  const utf8 = new TextEncoder().encode(str);
  const hexBytes = Array.from(utf8)
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('');
  const paddedHexLen = Math.ceil(hexBytes.length / 64) * 64;
  const fullHex = padUint(utf8.length) + hexBytes.padEnd(paddedHexLen, '0');
  return { hex: fullHex, byteLen: fullHex.length / 2 };
}

/** ABI-encode `NewsatoshiFairCurveFactory.createCurve(string,string,address,uint256,uint256,uint16,address,string)` (`0xc116e2f6`). */
export function encodeCreateFairCurve(params: {
  name: string;
  symbol: string;
  reserveToken?: string;
  virtualReserveRaw: bigint;
  virtualSupplyWei: bigint;
  swapFeeBps: number;
  feeSink?: string;
  metadataUri?: string;
}): string {
  const nameEnc = encodeAbiStringWord(params.name.trim());
  const symEnc = encodeAbiStringWord(params.symbol.trim());
  const uriEnc = encodeAbiStringWord((params.metadataUri ?? 'https://newsatoshi.org/launchpad/').trim());

  const headBytes = 8 * 32; // 256 bytes
  const off0 = headBytes;
  const off1 = off0 + nameEnc.byteLen;
  const off7 = off1 + symEnc.byteLen;

  return (
    '0xc116e2f6' +
    padUint(off0) +
    padUint(off1) +
    padAddress(params.reserveToken ?? BASE_USDC_ADDRESS) +
    padUint(params.virtualReserveRaw) +
    padUint(params.virtualSupplyWei) +
    padUint(params.swapFeeBps) +
    padAddress(params.feeSink ?? ZERO_ADDRESS) +
    padUint(off7) +
    nameEnc.hex +
    symEnc.hex +
    uriEnc.hex
  );
}

/** ABI-encode `FairCurvePool.buy(uint256 reserveIn, uint256 minTokensOut)` (`0xd6febde8`). */
export function encodeFairCurveBuy(reserveInRaw: bigint, minTokensOutWei = 0n): string {
  return `0xd6febde8${padUint(reserveInRaw)}${padUint(minTokensOutWei)}`;
}

/** ABI-encode `FairCurvePool.sell(uint256 tokensIn, uint256 minReserveOut)` (`0xd79875eb`). */
export function encodeFairCurveSell(tokensInWei: bigint, minReserveOutRaw = 0n): string {
  return `0xd79875eb${padUint(tokensInWei)}${padUint(minReserveOutRaw)}`;
}

/** ABI-encode `FairCurvePool.injectRevenue(uint256 amount)` (`0xe224ef38`). */
export function encodeFairCurveInjectRevenue(amountRaw: bigint): string {
  return `0xe224ef38${padUint(amountRaw)}`;
}

/** ABI-encode `FairCurvePool.claimRevenue()` (`0x564f4f76`). */
export function encodeFairCurveClaimRevenue(): string {
  return '0x564f4f76';
}

/** ABI-encode `FairCurvePool.claimableRevenue(address)` (`0xeacb482b`). */
export function encodeFairCurveClaimable(account: string): string {
  return `0xeacb482b${padAddress(account)}`;
}

/** ABI-encode `NewsatoshiPayWordChannel.openChannel(address,address,bytes32,uint256,uint32,uint64)` (`0xfbbd272c`). */
export function encodePayWordOpenChannel(params: {
  recipient: string;
  token?: string;
  rootHex: string;
  unitAmountRaw: bigint;
  maxSteps: number;
  expiryUnix: number;
}): string {
  return (
    '0xfbbd272c' +
    padAddress(params.recipient) +
    padAddress(params.token ?? BASE_USDC_ADDRESS) +
    padBytes32(params.rootHex) +
    padUint(params.unitAmountRaw) +
    padUint(params.maxSteps) +
    padUint(params.expiryUnix)
  );
}

/** ABI-encode `NewsatoshiPayWordChannel.claim(bytes32 channelId, uint32 steps, bytes32 preimage)` (`0xe0ab9c6b`). */
export function encodePayWordClaim(channelIdHex: string, steps: number, preimageHex: string): string {
  return `0xe0ab9c6b${padBytes32(channelIdHex)}${padUint(steps)}${padBytes32(preimageHex)}`;
}

/** ABI-encode `NewsatoshiPayWordChannel.closeAndRefund(bytes32 channelId)` (`0xa1fa8cf8`). */
export function encodePayWordCloseAndRefund(channelIdHex: string): string {
  return `0xa1fa8cf8${padBytes32(channelIdHex)}`;
}

/** ABI-encode `NewsatoshiPayWordChannel.computeChannelId(...)` (`0x5271855d`). */
export function encodePayWordComputeChannelId(params: {
  payer: string;
  recipient: string;
  token?: string;
  rootHex: string;
  unitAmountRaw: bigint;
  maxSteps: number;
  expiryUnix: number;
}): string {
  return (
    '0x5271855d' +
    padAddress(params.payer) +
    padAddress(params.recipient) +
    padAddress(params.token ?? BASE_USDC_ADDRESS) +
    padBytes32(params.rootHex) +
    padUint(params.unitAmountRaw) +
    padUint(params.maxSteps) +
    padUint(params.expiryUnix)
  );
}

/** Read a holder's claimable USDC dividends on a FairCurvePool via Base RPC. */
export async function readFairCurveClaimable(poolAddress: string, account: string): Promise<bigint> {
  const resHex = await rpcCall('base', 'eth_call', [
    { to: poolAddress, data: encodeFairCurveClaimable(account) },
    'latest',
  ]);
  const clean = strip0x(resHex);
  return clean.length >= 64 ? BigInt(`0x${clean.slice(0, 64)}`) : 0n;
}

/** Read exact Base Mainnet `computeChannelId` from `NewsatoshiPayWordChannel`. */
export async function readPayWordChannelId(params: {
  payer: string;
  recipient: string;
  token?: string;
  rootHex: string;
  unitAmountRaw: bigint;
  maxSteps: number;
  expiryUnix: number;
}): Promise<string> {
  const resHex = await rpcCall('base', 'eth_call', [
    { to: BASE_PAYWORD_CHANNEL_ADDRESS, data: encodePayWordComputeChannelId(params) },
    'latest',
  ]);
  const clean = strip0x(resHex);
  if (clean.length < 64) throw new Error('Failed to compute Base channelId');
  return `0x${clean.slice(0, 64)}`;
}

/** Generate a real CSPRNG SHA-256 PayWord hash chain strictly in the browser via WebCrypto. */
export async function generateBrowserPayWordChain(maxSteps: number): Promise<{
  seedHex: string;
  rootHex: string;
  chain: string[];
}> {
  const steps = Math.max(1, Math.min(2048, Math.floor(maxSteps)));
  const seed = new Uint8Array(32);
  window.crypto.getRandomValues(seed);
  const toHex = (u8: Uint8Array) =>
    '0x' + Array.from(u8).map((b) => b.toString(16).padStart(2, '0')).join('');
  const chain: string[] = new Array(steps + 1);
  let cur: Uint8Array = seed;
  chain[0] = toHex(cur);
  for (let i = 1; i <= steps; i += 1) {
    const ab = new ArrayBuffer(cur.byteLength);
    new Uint8Array(ab).set(cur);
    cur = new Uint8Array(await window.crypto.subtle.digest('SHA-256', ab));
    chain[i] = toHex(cur);
  }
  return {
    seedHex: chain[0],
    rootHex: chain[steps],
    chain,
  };
}
