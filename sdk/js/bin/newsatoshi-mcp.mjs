#!/usr/bin/env node
/**
 * newsatoshi-mcp v0.35.0 — Zero-Dependency Stdio MCP Bridge (46 Tools) & Multi-Chain Crypto/402 CLI
 *
 * Usage:
 *   npx -y github:theguysaccount/newsatoshi-commons
 *     Starts a stdio JSON-RPC 2.0 MCP bridge forwarding to https://commons.newsatoshi.org/mcp
 *     (all 46 zero-auth tools: Base/Optimism/Arbitrum O(1) PayWords, 4-chain SHA-256 HTLC swaps,
 *      Universal Payment URIs, Reserve-Price RFQ, Paid Agent Micro-APIs, EVM security audits,
 *      Bitcoin mempool oracle, BOLT-11 decoder, Base Uniswap V3 quoter, and Newsatoshi L1).
 *
 *   CLI Subcommands (zero API keys required):
 *     newsatoshi-mcp serve [--url <mcp-url>]
 *     newsatoshi-mcp tools
 *     newsatoshi-mcp status
 *     newsatoshi-mcp connectivity
 *     newsatoshi-mcp paid-catalog
 *     newsatoshi-mcp inspect <0xAddress> [--chain base|ethereum|arbitrum|optimism|polygon]
 *     newsatoshi-mcp inspect-channel <0xChannelId> [--chain base|optimism|arbitrum]
 *     newsatoshi-mcp inspect-htlc <0xSwapId> [--chain base|optimism|arbitrum|polygon]
 *     newsatoshi-mcp pay-uri <recipient> <amount> [--rail all|base|optimism|arbitrum|polygon|bitcoin|lightning|solana|newsatoshi] [--asset USDC] [--memo "..."]
 *     newsatoshi-mcp rfq <loveAmount> <minReserveUsd> <sellerAddr> [--rail base_usdc] [--hours 24]
 *     newsatoshi-mcp btc-fees
 *     newsatoshi-mcp decode-bolt11 <lnbc...>
 *     newsatoshi-mcp quote-base <ETH|USDC|WETH|wLOVE> <ETH|USDC|WETH|wLOVE> <amount>
 *     newsatoshi-mcp verify-tx <0xTxHash> [--chain base|ethereum|arbitrum|optimism|polygon]
 *     newsatoshi-mcp payword-demo [0xAddress] [--chain base]
 *     newsatoshi-mcp help
 */

import fs from "node:fs";
import readline from "node:readline";
import { pathToFileURL } from "node:url";
import {
  DEFAULT_COMMONS_URL,
  NewsatoshiMcpClient,
  generatePayWordChain,
  fetchWithNewsatoshi402,
} from "../newsatoshi-402.mjs";

const MCP_URL =
  process.env.NEWSATOSHI_MCP_URL ||
  `${(process.env.NEWSATOSHI_COMMONS_URL || DEFAULT_COMMONS_URL).replace(/\/$/, "")}/mcp`;

const COMMONS_BASE = MCP_URL.replace(/\/mcp$/, "");

function parseFlag(args, flag, fallback = undefined) {
  const idx = args.indexOf(flag);
  if (idx >= 0 && idx + 1 < args.length) return args[idx + 1];
  return fallback;
}

function printHelp() {
  console.log(`newsatoshi-mcp v0.37.0 — Zero-API-Key Universal Interop & MCP Bridge (52 Tools) + O(1) PayWords CLI

STDIO MCP BRIDGE (default when invoked with no subcommand):
  npx -y github:theguysaccount/newsatoshi-commons
  newsatoshi-mcp serve [--url https://commons.newsatoshi.org/mcp]

ZERO-API-KEY CLI COMMANDS:
  newsatoshi-mcp tools
  newsatoshi-mcp status
  newsatoshi-mcp connectivity
  newsatoshi-mcp interop-matrix
  newsatoshi-mcp erc7683 <originChain> <userEvm> <destNs1> [--token USDC] [--amount 100000000]
  newsatoshi-mcp universal-htlc <srcRail> <dstRail> <recipient> [--amount "100 USDC"]
  newsatoshi-mcp iso20022 <ticker> <amountDecimal> [--debtor CHASUS33XXX] [--creditor FRBKUS44XXX]
  newsatoshi-mcp simulate-interop
  newsatoshi-mcp simulate-factory [--ticker BANKUSD]
  newsatoshi-mcp simulate-order
  newsatoshi-mcp paid-catalog
  newsatoshi-mcp inspect <0xContract> [--chain base|ethereum|arbitrum|optimism|polygon]
  newsatoshi-mcp inspect-channel <0xChannelId> [--chain base|optimism|arbitrum]
  newsatoshi-mcp inspect-htlc <0xSwapId> [--chain base|optimism|arbitrum|polygon]
  newsatoshi-mcp pay-uri <recipient> <amount> [--rail all|base|optimism|arbitrum|polygon|bitcoin|solana|newsatoshi] [--asset USDC] [--memo "Order"]
  newsatoshi-mcp rfq <loveAmount> <minReserveUsd> <sellerAddr> [--rail base_usdc] [--hours 24]
  newsatoshi-mcp btc-fees
  newsatoshi-mcp decode-bolt11 <bolt11_invoice>
  newsatoshi-mcp quote-base <tokenIn> <tokenOut> <amount>
  newsatoshi-mcp verify-tx <0xTxHash> [--chain base|ethereum|arbitrum|optimism|polygon]
  newsatoshi-mcp payword-demo [0xContract] [--chain base]

DIRECT STREAMABLE HTTP MCP URL:
  ${MCP_URL}
`);
}

export async function forwardJsonRpcLine(line, mcpUrl = MCP_URL, fetchImpl = fetch) {
  const trimmed = line.trim();
  if (!trimmed) return null;
  let parsed;
  try {
    parsed = JSON.parse(trimmed);
  } catch (err) {
    return JSON.stringify({
      jsonrpc: "2.0",
      id: null,
      error: { code: -32700, message: `Parse error: ${err.message}` },
    });
  }

  const isNotification =
    parsed && typeof parsed === "object" && !Array.isArray(parsed) && parsed.id === undefined;

  try {
    const resp = await fetchImpl(mcpUrl, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Accept: "application/json",
        "User-Agent": "newsatoshi-mcp-stdio/0.35.0",
      },
      body: trimmed,
    });
    const text = await resp.text();
    if (isNotification) return null;
    if (!resp.ok) {
      return JSON.stringify({
        jsonrpc: "2.0",
        id: parsed.id ?? null,
        error: {
          code: -32000,
          message: `Upstream MCP HTTP ${resp.status}: ${text.slice(0, 300)}`,
        },
      });
    }
    return text.trim() || null;
  } catch (err) {
    if (isNotification) return null;
    return JSON.stringify({
      jsonrpc: "2.0",
      id: parsed.id ?? null,
      error: { code: -32001, message: `Upstream MCP transport error: ${err.message}` },
    });
  }
}

async function runStdioBridge(mcpUrl) {
  const rl = readline.createInterface({
    input: process.stdin,
    crlfDelay: Infinity,
    terminal: false,
  });

  let queue = Promise.resolve();
  rl.on("line", (line) => {
    queue = queue.then(async () => {
      const out = await forwardJsonRpcLine(line, mcpUrl);
      if (out !== null) {
        process.stdout.write(`${out}\n`);
      }
    });
  });

  await new Promise((resolve) => rl.on("close", resolve));
  await queue;
}

function unwrapToolResult(res) {
  if (res && Array.isArray(res.content) && res.content[0]?.text) {
    try {
      return JSON.parse(res.content[0].text);
    } catch {
      return res.content[0].text;
    }
  }
  return res;
}

async function runCli(argv) {
  const cmd = argv[0];
  if (!cmd || cmd === "serve") {
    const url = parseFlag(argv, "--url", MCP_URL);
    await runStdioBridge(url);
    return;
  }
  if (cmd === "help" || cmd === "--help" || cmd === "-h") {
    printHelp();
    return;
  }

  const client = new NewsatoshiMcpClient(parseFlag(argv, "--url", MCP_URL));

  if (cmd === "tools") {
    const list = await client.listTools();
    console.log(
      JSON.stringify(
        (list.tools || []).map((t) => ({ name: t.name, description: t.description })),
        null,
        2
      )
    );
    return;
  }

  if (cmd === "status") {
    const res = unwrapToolResult(await client.callTool("newsatoshi_get_network_status", {}));
    console.log(JSON.stringify(res, null, 2));
    return;
  }

  if (cmd === "connectivity") {
    const res = unwrapToolResult(await client.callTool("newsatoshi_get_crosschain_connectivity", {}));
    console.log(JSON.stringify(res, null, 2));
    return;
  }

  if (cmd === "paid-catalog") {
    const res = unwrapToolResult(await client.callTool("newsatoshi_get_paid_api_catalog", {}));
    console.log(JSON.stringify(res, null, 2));
    return;
  }

  if (cmd === "inspect") {
    const address = argv[1];
    if (!address) throw new Error("Usage: newsatoshi-mcp inspect <0xAddress> [--chain base]");
    const chain = parseFlag(argv, "--chain", "base");
    const res = unwrapToolResult(await client.inspectEvmContract(address, chain));
    console.log(JSON.stringify(res, null, 2));
    return;
  }

  if (cmd === "inspect-channel") {
    const channelId = argv[1];
    if (!channelId) throw new Error("Usage: newsatoshi-mcp inspect-channel <0xChannelId> [--chain base|optimism|arbitrum]");
    const chain = parseFlag(argv, "--chain", "base");
    const res = unwrapToolResult(
      await client.callTool("newsatoshi_inspect_multichain_payword_channel", { chain, channel_id: channelId })
    );
    console.log(JSON.stringify(res, null, 2));
    return;
  }

  if (cmd === "inspect-htlc") {
    const swapId = argv[1];
    if (!swapId) throw new Error("Usage: newsatoshi-mcp inspect-htlc <0xSwapId> [--chain base|optimism|arbitrum|polygon]");
    const chain = parseFlag(argv, "--chain", "base");
    const res = unwrapToolResult(
      await client.callTool("newsatoshi_inspect_evm_htlc", { chain, swap_id: swapId })
    );
    console.log(JSON.stringify(res, null, 2));
    return;
  }

  if (cmd === "pay-uri") {
    const [, recipient, amountStr] = argv;
    if (!recipient || !amountStr) {
      throw new Error("Usage: newsatoshi-mcp pay-uri <recipient> <amount> [--rail all|base|optimism|arbitrum|polygon|bitcoin|solana|newsatoshi]");
    }
    const rail = parseFlag(argv, "--rail", "all");
    const asset = parseFlag(argv, "--asset");
    const memo = parseFlag(argv, "--memo");
    const res = unwrapToolResult(
      await client.callTool("newsatoshi_build_universal_payment_uri", {
        rail,
        recipient,
        amount: Number(amountStr),
        ...(asset ? { asset } : {}),
        ...(memo ? { memo } : {}),
      })
    );
    console.log(JSON.stringify(res, null, 2));
    return;
  }

  if (cmd === "rfq") {
    const [, loveStr, minUsdStr, sellerAddr] = argv;
    if (!loveStr || !minUsdStr || !sellerAddr) {
      throw new Error("Usage: newsatoshi-mcp rfq <loveAmount> <minReserveUsd> <sellerAddr> [--rail base_usdc] [--hours 24]");
    }
    const settlement_rail = parseFlag(argv, "--rail", "base_usdc");
    const expiry_hours = Number(parseFlag(argv, "--hours", "24"));
    const res = unwrapToolResult(
      await client.callTool("newsatoshi_compile_reserve_price_rfq", {
        love_amount: Number(loveStr),
        min_reserve_price_usd: Number(minUsdStr),
        settlement_rail,
        seller_settlement_address: sellerAddr,
        expiry_hours,
      })
    );
    console.log(JSON.stringify(res, null, 2));
    return;
  }

  if (cmd === "btc-fees") {
    const res = unwrapToolResult(await client.bitcoinMempoolOracle());
    console.log(JSON.stringify(res, null, 2));
    return;
  }

  if (cmd === "decode-bolt11") {
    const invoice = argv[1];
    if (!invoice) throw new Error("Usage: newsatoshi-mcp decode-bolt11 <bolt11_invoice>");
    const res = unwrapToolResult(await client.decodeBolt11(invoice));
    console.log(JSON.stringify(res, null, 2));
    return;
  }

  if (cmd === "quote-base") {
    const [, tokenIn, tokenOut, amount] = argv;
    if (!tokenIn || !tokenOut || !amount) {
      throw new Error("Usage: newsatoshi-mcp quote-base <tokenIn> <tokenOut> <amount>");
    }
    const res = unwrapToolResult(await client.quoteBaseUniswapV3(tokenIn, tokenOut, amount));
    console.log(JSON.stringify(res, null, 2));
    return;
  }

  if (cmd === "verify-tx") {
    const txHash = argv[1];
    if (!txHash) throw new Error("Usage: newsatoshi-mcp verify-tx <0xTxHash> [--chain base]");
    const chain = parseFlag(argv, "--chain", "base");
    const res = unwrapToolResult(await client.verifyTxReceipt(txHash, chain));
    console.log(JSON.stringify(res, null, 2));
    return;
  }

  if (cmd === "payword-demo") {
    const address =
      argv[1] && !argv[1].startsWith("--")
        ? argv[1]
        : "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913";
    const chain = parseFlag(argv, "--chain", "base");
    const wallet = await generatePayWordChain(10, 1_000);
    const t0 = performance.now();
    const resp = await fetchWithNewsatoshi402(
      `${COMMONS_BASE}/api/x402/evm-security-scan`,
      wallet,
      {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ chain, address }),
      },
      1
    );
    const elapsedMs = Math.round(performance.now() - t0);
    const data = await resp.json();
    console.log(
      JSON.stringify(
        {
          http_status: resp.status,
          roundtrip_ms: elapsedMs,
          payword_tip: wallet.tipHex,
          ticks_spent: wallet.spentTicks,
          result: data,
        },
        null,
        2
      )
    );
    return;
  }

  if (cmd === "interop-matrix") {
    const resp = await fetch(`${COMMONS_BASE}/api/v2/interop/matrix`);
    console.log(JSON.stringify(await resp.json(), null, 2));
    return;
  }

  if (cmd === "erc7683") {
    const [, origin_chain = "base", user_evm_address = "0x1111222233334444555566667777888899990000", destination_ns1 = "ns1qinteroptreasury0000000000000000001"] = argv;
    const input_token = parseFlag(argv, "--token", "USDC");
    const input_amount_units = Number(parseFlag(argv, "--amount", "100000000"));
    const res = unwrapToolResult(
      await client.callTool("newsatoshi_compile_erc7683_intent", {
        origin_chain,
        user_evm_address,
        input_token,
        input_amount_units,
        destination_ns1,
      })
    );
    console.log(JSON.stringify(res, null, 2));
    return;
  }

  if (cmd === "universal-htlc") {
    const [, source_rail = "cosmos_ibc", dest_rail = "newsatoshi_l1", recipient = "ns1qrecipient00000000000000000001"] = argv;
    const amount_display = parseFlag(argv, "--amount", "100 USDC");
    const res = unwrapToolResult(
      await client.callTool("newsatoshi_compile_universal_htlc", {
        source_rail,
        dest_rail,
        recipient,
        amount_display,
      })
    );
    console.log(JSON.stringify(res, null, 2));
    return;
  }

  if (cmd === "iso20022") {
    const [, stablecoin_ticker = "BANKUSD", amount_decimal = "250000.00"] = argv;
    const debtor_bic = parseFlag(argv, "--debtor", "CHASUS33XXX");
    const creditor_bic = parseFlag(argv, "--creditor", "FRBKUS44XXX");
    const res = unwrapToolResult(
      await client.callTool("newsatoshi_compile_iso20022_stablecoin_bridge", {
        stablecoin_ticker,
        amount_decimal,
        debtor_bic,
        creditor_bic,
      })
    );
    console.log(JSON.stringify(res, null, 2));
    return;
  }

  if (cmd === "simulate-interop") {
    const res = unwrapToolResult(await client.callTool("newsatoshi_simulate_universal_interop", {}));
    console.log(JSON.stringify(res, null, 2));
    return;
  }

  if (cmd === "simulate-factory") {
    const ticker = parseFlag(argv, "--ticker", "BANKUSD");
    const res = unwrapToolResult(await client.callTool("newsatoshi_simulate_sovereign_factory", { ticker }));
    console.log(JSON.stringify(res, null, 2));
    return;
  }

  if (cmd === "simulate-order") {
    const res = unwrapToolResult(await client.callTool("newsatoshi_simulate_proof_of_order_vault", {}));
    console.log(JSON.stringify(res, null, 2));
    return;
  }

  throw new Error(`Unknown command "${cmd}". Run "newsatoshi-mcp help" for usage.`);
}

const isMain = (() => {
  if (!process.argv[1]) return false;
  try {
    return import.meta.url === pathToFileURL(fs.realpathSync(process.argv[1])).href;
  } catch {
    return import.meta.url === pathToFileURL(process.argv[1]).href;
  }
})();

if (isMain) {
  runCli(process.argv.slice(2)).catch((err) => {
    console.error(`newsatoshi-mcp error: ${err.message}`);
    process.exit(1);
  });
}
