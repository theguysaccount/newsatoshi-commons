// TypeScript definitions for @newsatoshi/402 (zero-dependency multi-chain HTTP 402 PayWords & MCP SDK)

export const DEFAULT_COMMONS_URL: string;
export const DEFAULT_MAINNET_SEED_URL: string;
export const DEFAULT_TESTNET_SEED_URL: string;
export const MAX_PAYWORD_TICKS: number;
export const MAX_PAYWORD_STEP_JUMP: number;
export const CANONICAL_EVM_HTLC: string;
export const CANONICAL_BACKING_VAULT: string;
export const BASE_CHAIN_ID: number;
export const DEFAULT_BASE_RPC_URL: string;
export const BASE_USDC_ADDRESS: string;
export const BASE_PAYWORD_CHANNEL_ADDRESS: string;
export const BASE_FAIR_CURVE_FACTORY_ADDRESS: string;
export const BASE_DEMO_CURVE_POOL_ADDRESS: string;

export interface EvmPayWordChainConfig {
  readonly chainId: number;
  readonly caip2: string;
  readonly name: string;
  readonly rpcUrl: string;
  readonly usdc: string;
  readonly paywordChannel: string;
  readonly htlc: string;
  readonly backingVault: string;
  readonly explorer: string;
  readonly sourcifyUrl: string;
}

export const EVM_PAYWORD_CHAINS: Readonly<{
  base: EvmPayWordChainConfig;
  optimism: EvmPayWordChainConfig;
  arbitrum: EvmPayWordChainConfig;
}>;

export interface PayWordVoucher {
  index: number;
  tokenHex: string;
  tipHex: string;
  headerValue: string;
  remaining: number;
}

export interface PayWordWallet {
  maxTicks: number;
  unitPriceBaseUnits: number;
  tipHex: string;
  nextIndex: number;
  popNextVoucher(ticks?: number): PayWordVoucher;
  exportState(): Record<string, unknown>;
}

export function generatePayWordChain(
  maxTicks?: number,
  unitPriceBaseUnits?: number,
  seedHex?: string,
): Promise<PayWordWallet>;

export function verifyPayWordToken(
  tipHex: string,
  tokenHex: string,
  ticks: number,
): Promise<boolean>;

export function parsePayWordHeader(
  headerValue: string,
): { tipHex: string; tokenHex: string; ticks: number } | null;

export interface AutoPayFetchOptions {
  wallet: PayWordWallet;
  chain?: "base" | "optimism" | "arbitrum";
  channelId?: string;
  ticksPerCall?: number;
  fetchImpl?: typeof fetch;
}

export function createAutoPayFetch(
  options: AutoPayFetchOptions,
): (input: RequestInfo | URL, init?: RequestInit) => Promise<Response>;

export function fetchWithNewsatoshi402(
  url: string,
  paywordWallet: PayWordWallet,
  fetchOptions?: RequestInit,
  ticksPerCall?: number,
): Promise<Response>;

export interface BasePayWordGateOptions {
  recipientAddress: string;
  unitAmountRaw?: number | bigint;
  tokenAddress?: string;
  chain?: "base" | "optimism" | "arbitrum";
  rpcUrl?: string;
  channelContract?: string;
  verifyOnChain?: boolean;
  store?: {
    get(key: string): Promise<Record<string, unknown> | null> | Record<string, unknown> | null;
    set(key: string, value: Record<string, unknown>): Promise<void> | void;
  };
}

export interface BasePayWordGateResult {
  allowed: boolean;
  status: number;
  headers: Record<string, string>;
  body: Record<string, unknown>;
  channelId?: string;
  index?: number;
  tokenHex?: string;
}

export function createBasePayWordGate(options: BasePayWordGateOptions): {
  verifyRequest(headers: Headers | Record<string, string | undefined>): Promise<BasePayWordGateResult>;
  middleware(): (req: unknown, res: unknown, next: () => void) => Promise<void>;
};

export function createPayWordGate(options: Record<string, unknown>): Record<string, unknown>;
export function createDual402Gate(options: Record<string, unknown>): Record<string, unknown>;

export function fetchBasePayWordChannel(
  channelIdHex: string,
  rpcUrl?: string,
  channelContract?: string,
): Promise<Record<string, unknown>>;

export function encodeErc20ApproveCalldata(spender: string, amount: bigint | number | string): string;
export function encodeBaseOpenChannelCalldata(params: {
  recipient: string;
  token: string;
  initialRoot: string;
  unitAmount: bigint | number | string;
  maxSteps: bigint | number;
  expiryUnix: bigint | number;
}): string;
export function encodeBaseClaimCalldata(params: {
  channelIdHex: string;
  steps: bigint | number;
  preimageHex: string;
}): string;
export function encodeBaseCloseAndRefundCalldata(channelIdHex: string): string;
export function encodeBaseRefundCalldata(channelIdHex: string): string;

export class NewsatoshiMcpClient {
  constructor(url?: string, fetchImpl?: typeof fetch);
  initialize(): Promise<Record<string, unknown>>;
  listTools(): Promise<Array<Record<string, unknown>>>;
  callTool(name: string, args?: Record<string, unknown>): Promise<Record<string, unknown>>;
}
