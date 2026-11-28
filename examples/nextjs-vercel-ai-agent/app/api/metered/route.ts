// Next.js App Router metered API endpoint using O(1) SHA-256 PayWords (Base / Optimism / Arbitrum)
import { createBasePayWordGate, EVM_PAYWORD_CHAINS } from "newsatoshi-mcp";

const watermarkStore = new Map<string, Record<string, unknown>>();
const chainKey = (process.env.PAYWORDS_CHAIN as "base" | "optimism" | "arbitrum") || "base";
const chain = EVM_PAYWORD_CHAINS[chainKey] || EVM_PAYWORD_CHAINS.base;

const gate = createBasePayWordGate({
  recipientAddress: process.env.PAYWORDS_RECIPIENT || "0x1744535521EB420be9700475400455a4Ad0EBEa2",
  unitAmountRaw: 1000n, // $0.001 USDC per call
  tokenAddress: chain.usdc,
  chain: chainKey,
  rpcUrl: chain.rpcUrl,
  channelContract: chain.paywordChannel,
  verifyOnChain: true,
  store: {
    get: (k) => watermarkStore.get(k) ?? null,
    set: (k, v) => { watermarkStore.set(k, v); },
  },
});

export async function POST(req: Request): Promise<Response> {
  const verdict = await gate.verifyRequest(req.headers);
  if (!verdict.allowed) {
    return new Response(JSON.stringify(verdict.body), {
      status: verdict.status,
      headers: { "Content-Type": "application/json", ...verdict.headers },
    });
  }

  const body = await req.json().catch(() => ({}));
  return Response.json({
    ok: true,
    chain: chain.name,
    channel_id: verdict.channelId,
    step: verdict.index,
    echo: body,
  });
}
