// Zero-gas O(1) SHA-256 PayWords Metered API on Cloudflare Workers
// Supports live Sourcify-verified NewsatoshiPayWordChannel on Base, Optimism, and Arbitrum (0% protocol fee).
import { createBasePayWordGate, EVM_PAYWORD_CHAINS } from "newsatoshi-mcp";

interface Env {
  PAYWORDS_CHAIN?: "base" | "optimism" | "arbitrum";
  PAYWORDS_RECIPIENT: string;
  PAYWORDS_UNIT_RAW?: string;
}

const memStore = new Map<string, Record<string, unknown>>();

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const url = new URL(request.url);
    if (url.pathname === "/health") {
      return Response.json({ ok: true, chains: Object.keys(EVM_PAYWORD_CHAINS) });
    }

    const chainKey = env.PAYWORDS_CHAIN || "base";
    const chain = EVM_PAYWORD_CHAINS[chainKey] || EVM_PAYWORD_CHAINS.base;
    const gate = createBasePayWordGate({
      recipientAddress: env.PAYWORDS_RECIPIENT,
      unitAmountRaw: BigInt(env.PAYWORDS_UNIT_RAW || "1000"), // $0.001 USDC
      tokenAddress: chain.usdc,
      chain: chainKey,
      rpcUrl: chain.rpcUrl,
      channelContract: chain.paywordChannel,
      verifyOnChain: true,
      store: {
        get: (k) => memStore.get(k) ?? null,
        set: (k, v) => { memStore.set(k, v); },
      },
    });

    const result = await gate.verifyRequest(request.headers);
    if (!result.allowed) {
      return new Response(JSON.stringify(result.body, null, 2), {
        status: result.status,
        headers: { "Content-Type": "application/json", ...result.headers },
      });
    }

    // Paid request verified in ~10 microseconds at $0.000000 per-call gas!
    return Response.json({
      ok: true,
      settled_via: `Newsatoshi O(1) PayWords (${chain.name})`,
      channel_id: result.channelId,
      voucher_index: result.index,
      data: { message: "Metered edge compute delivered", ts: Date.now() },
    });
  },
};
