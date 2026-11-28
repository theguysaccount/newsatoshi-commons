// Autonomous AI Agent client (works with OpenAI Agents SDK, Vercel AI SDK, Claude Tools, Node 18+)
// Automatically handles HTTP 402 -> pops next O(1) SHA-256 PayWord voucher -> retries -> 200 OK.
import { generatePayWordChain, createAutoPayFetch } from "newsatoshi-mcp";

const TARGET_URL = process.env.PAID_API_URL || "https://commons.newsatoshi.org/api/v2/paywords/try-402";
const CHAIN = (process.env.PAYWORDS_CHAIN as "base" | "optimism" | "arbitrum") || "base";
const CHANNEL_ID = process.env.PAYWORDS_CHANNEL_ID || ("0x" + "00".repeat(32));

async function main() {
  const wallet = await generatePayWordChain(1000, 1000);
  const payFetch = createAutoPayFetch({ wallet, chain: CHAIN, channelId: CHANNEL_ID });

  for (let i = 1; i <= 3; i++) {
    const res = await payFetch(TARGET_URL, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ call: i }),
    });
    console.log(`Call #${i} -> HTTP ${res.status}`, await res.json());
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
