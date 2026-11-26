import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  AlertTriangle,
  ArrowDownUp,
  ArrowUpRight,
  CheckCircle2,
  Loader2,
  Plus,
  Settings2,
  Wallet,
} from 'lucide-react';
import {
  BASE_USDC_ADDRESS,
  BASE_UNISWAP_ROUTER,
  BASE_WLOVE_ADDRESS,
  BASE_WLOVE_POOL,
  EVM_CHAINS,
  buildBaseSwapTransaction,
  encodeApprove,
  fetchBaseWlovePoolSnapshot,
  formatCompactToken,
  formatUnits,
  parseUnits,
  quoteBaseSwap,
  readErc20Allowance,
  type BasePoolSnapshot,
  type BaseSwapAsset,
  type BaseSwapQuote,
} from '../wallet/evm';
import { useWallet } from '../wallet/WalletContext';

interface InstantSwapCardProps {
  compact?: boolean;
  onNotice?: (kind: 'success' | 'error', message: string) => void;
  onOpenBridge?: () => void;
}

const DECIMALS: Record<BaseSwapAsset, number> = {
  ETH: 18,
  USDC: 6,
  wLOVE: 18,
};

export function InstantSwapCard({
  compact = false,
  onNotice,
  onOpenBridge,
}: InstantSwapCardProps) {
  const {
    evmAddress,
    evmChainId,
    balances,
    openModal,
    switchEvmChain,
    sendEvmTx,
    waitForEvmReceipt,
    addWloveToWallet,
    refreshBalances,
  } = useWallet();

  // Mode: buying wLOVE (ETH/USDC -> wLOVE) or selling wLOVE (wLOVE -> ETH/USDC)
  const [payAsset, setPayAsset] = useState<BaseSwapAsset>('ETH');
  const [receiveAsset, setReceiveAsset] = useState<BaseSwapAsset>('wLOVE');
  const [amountInput, setAmountInput] = useState<string>('0.001');
  const [slippageBps, setSlippageBps] = useState<number>(200); // 2.0% default for genesis pool smoothness
  const [settingsOpen, setSettingsOpen] = useState<boolean>(false);

  const [pool, setPool] = useState<BasePoolSnapshot | null>(null);
  const [quote, setQuote] = useState<BaseSwapQuote | null>(null);
  const [quoting, setQuoting] = useState<boolean>(false);
  const [quoteError, setQuoteError] = useState<string | null>(null);

  const [allowance, setAllowance] = useState<bigint | null>(null);
  const [busyStep, setBusyStep] = useState<null | 'switch' | 'approve' | 'swap'>(null);
  const [lastTxHash, setLastTxHash] = useState<string | null>(null);
  const [lastTxStatus, setLastTxStatus] = useState<'pending' | 'confirmed' | 'failed' | null>(null);

  const quoteReqId = useRef(0);

  // Fetch live Base Uniswap V3 pool reserves once & refresh every 30s
  useEffect(() => {
    let active = true;
    const loadPool = async () => {
      try {
        const snap = await fetchBaseWlovePoolSnapshot();
        if (active) setPool(snap);
      } catch {
        // silent
      }
    };
    void loadPool();
    const id = window.setInterval(() => void loadPool(), 30000);
    return () => {
      active = false;
      window.clearInterval(id);
    };
  }, []);

  const amountInRaw = useMemo(
    () => parseUnits(amountInput, DECIMALS[payAsset]),
    [amountInput, payAsset]
  );

  // Live debounced QuoterV2 call on Base Mainnet
  useEffect(() => {
    if (!amountInRaw || amountInRaw <= 0n) {
      setQuote(null);
      setQuoteError(null);
      setQuoting(false);
      return;
    }
    const reqId = ++quoteReqId.current;
    setQuoting(true);
    setQuoteError(null);
    const timer = window.setTimeout(() => {
      void (async () => {
        try {
          const q = await quoteBaseSwap(payAsset, receiveAsset, amountInRaw, pool);
          if (quoteReqId.current === reqId) {
            setQuote(q);
            setQuoteError(null);
          }
        } catch (err) {
          if (quoteReqId.current === reqId) {
            setQuote(null);
            setQuoteError(
              err instanceof Error ? err.message : 'Unable to fetch live Base quote'
            );
          }
        } finally {
          if (quoteReqId.current === reqId) setQuoting(false);
        }
      })();
    }, 240);

    return () => window.clearTimeout(timer);
  }, [amountInRaw, payAsset, receiveAsset, pool]);

  // Check ERC-20 allowance whenever selling USDC or wLOVE while connected
  const refreshAllowance = useCallback(async () => {
    if (!evmAddress || payAsset === 'ETH') {
      setAllowance(null);
      return;
    }
    const tokenAddr = payAsset === 'USDC' ? BASE_USDC_ADDRESS : BASE_WLOVE_ADDRESS;
    try {
      const a = await readErc20Allowance('base', tokenAddr, evmAddress, BASE_UNISWAP_ROUTER);
      setAllowance(a);
    } catch {
      setAllowance(0n);
    }
  }, [evmAddress, payAsset]);

  useEffect(() => {
    void refreshAllowance();
  }, [refreshAllowance]);

  const payBalanceRaw = useMemo(() => {
    if (!evmAddress) return 0n;
    if (payAsset === 'ETH') return balances.baseEthWei;
    if (payAsset === 'USDC') return balances.baseUsdcRaw;
    return balances.baseWloveWei;
  }, [evmAddress, payAsset, balances]);

  const receiveBalanceRaw = useMemo(() => {
    if (!evmAddress) return 0n;
    if (receiveAsset === 'ETH') return balances.baseEthWei;
    if (receiveAsset === 'USDC') return balances.baseUsdcRaw;
    return balances.baseWloveWei;
  }, [evmAddress, receiveAsset, balances]);

  const isOnBase = evmChainId === EVM_CHAINS.base.chainId;
  const insufficientBalance =
    Boolean(evmAddress) && amountInRaw !== null && amountInRaw > 0n && amountInRaw > payBalanceRaw;
  const needsApproval =
    Boolean(evmAddress) &&
    isOnBase &&
    payAsset !== 'ETH' &&
    amountInRaw !== null &&
    amountInRaw > 0n &&
    (allowance === null || allowance < amountInRaw);

  const handleFlip = () => {
    const nextPay = receiveAsset;
    const nextReceive = payAsset;
    setPayAsset(nextPay);
    setReceiveAsset(nextReceive);
    setQuote(null);
    setQuoteError(null);
    if (nextPay === 'ETH') setAmountInput('0.001');
    else if (nextPay === 'USDC') setAmountInput('1');
    else setAmountInput('100000');
  };

  const handleSelectCounterAsset = (asset: 'ETH' | 'USDC') => {
    if (payAsset === 'wLOVE') {
      setReceiveAsset(asset);
    } else {
      setPayAsset(asset);
      setReceiveAsset('wLOVE');
      setAmountInput(asset === 'ETH' ? '0.001' : '1');
    }
    setQuote(null);
  };

  const handleSetMax = () => {
    if (!evmAddress) return;
    if (payAsset === 'ETH') {
      // Leave tiny gas reserve (~0.00012 ETH on Base)
      const gasReserve = 120_000_000_000_000n;
      const spendable = payBalanceRaw > gasReserve ? payBalanceRaw - gasReserve : 0n;
      setAmountInput(formatUnits(spendable, 18, 6).replace(/,/g, ''));
    } else if (payAsset === 'USDC') {
      setAmountInput(formatUnits(payBalanceRaw, 6, 6).replace(/,/g, ''));
    } else {
      setAmountInput(formatUnits(payBalanceRaw, 18, 4).replace(/,/g, ''));
    }
  };

  const handlePresetPercent = (pct: number) => {
    if (!evmAddress || payBalanceRaw <= 0n) return;
    const raw = (payBalanceRaw * BigInt(pct)) / 100n;
    setAmountInput(formatUnits(raw, DECIMALS[payAsset], payAsset === 'wLOVE' ? 2 : 5).replace(/,/g, ''));
  };

  const handlePrimaryAction = async () => {
    if (!evmAddress) {
      openModal();
      return;
    }
    if (!isOnBase) {
      setBusyStep('switch');
      try {
        const switched = await switchEvmChain('base');
        if (switched) onNotice?.('success', 'Switched wallet to Base Mainnet');
      } finally {
        setBusyStep(null);
      }
      return;
    }
    if (!amountInRaw || amountInRaw <= 0n || !quote) return;

    if (needsApproval) {
      setBusyStep('approve');
      try {
        const tokenAddr = payAsset === 'USDC' ? BASE_USDC_ADDRESS : BASE_WLOVE_ADDRESS;
        // Approve exact needed amount (safe non-infinite approval)
        const txHash = await sendEvmTx({
          to: tokenAddr,
          data: encodeApprove(BASE_UNISWAP_ROUTER, amountInRaw),
          value: '0x0',
        });
        onNotice?.('success', `Approving ${payAsset} on Base…`);
        const status = await waitForEvmReceipt('base', txHash);
        await refreshAllowance();
        if (status === 'success') {
          onNotice?.('success', `${payAsset} approved — click Swap to execute`);
        } else {
          onNotice?.('error', `${payAsset} approval did not confirm`);
        }
      } catch (err) {
        onNotice?.('error', err instanceof Error ? err.message : 'Approval rejected');
      } finally {
        setBusyStep(null);
      }
      return;
    }

    setBusyStep('swap');
    setLastTxHash(null);
    setLastTxStatus(null);
    try {
      const minOut = (quote.amountOutRaw * BigInt(10000 - slippageBps)) / 10000n;
      const tx = buildBaseSwapTransaction({
        fromAsset: payAsset,
        toAsset: receiveAsset,
        amountInRaw,
        minAmountOutRaw: minOut > 0n ? minOut : 1n,
        recipient: evmAddress,
      });
      const txHash = await sendEvmTx({
        to: tx.to,
        data: tx.data,
        value: tx.valueHex,
      });
      setLastTxHash(txHash);
      setLastTxStatus('pending');
      onNotice?.('success', `Swap submitted on Base (${txHash.slice(0, 10)}…)`);
      const receipt = await waitForEvmReceipt('base', txHash);
      if (receipt === 'success') {
        setLastTxStatus('confirmed');
        onNotice?.('success', `Swapped ${amountInput} ${payAsset} → ${receiveAsset} on Base!`);
        void refreshBalances();
        void fetchBaseWlovePoolSnapshot().then(setPool).catch(() => {});
      } else if (receipt === 'reverted') {
        setLastTxStatus('failed');
        onNotice?.('error', 'Swap reverted on-chain — try higher slippage or smaller size');
      }
    } catch (err) {
      onNotice?.('error', err instanceof Error ? err.message : 'Swap cancelled');
    } finally {
      setBusyStep(null);
    }
  };

  const formattedOut = quote
    ? receiveAsset === 'wLOVE'
      ? formatUnits(quote.amountOutRaw, 18, 2)
      : formatUnits(quote.amountOutRaw, DECIMALS[receiveAsset], receiveAsset === 'ETH' ? 6 : 4)
    : '';

  const minReceived = quote
    ? (quote.amountOutRaw * BigInt(10000 - slippageBps)) / 10000n
    : null;

  const highImpact = (quote?.priceImpactPct ?? 0) >= 12;

  const primaryLabel = useMemo(() => {
    if (busyStep === 'switch') return 'Switching to Base…';
    if (busyStep === 'approve') return `Approving ${payAsset}…`;
    if (busyStep === 'swap') return 'Swapping on Base…';
    if (!evmAddress) return 'Connect Wallet to Swap';
    if (!isOnBase) return 'Switch Wallet to Base';
    if (!amountInRaw || amountInRaw <= 0n) return 'Enter an amount';
    if (insufficientBalance) return `Insufficient ${payAsset} on Base`;
    if (quoting) return 'Fetching live quote…';
    if (quoteError || !quote) return 'Try a smaller amount';
    if (needsApproval) return `1. Approve ${payAsset} on Base`;
    return `Swap ${payAsset} for ${receiveAsset}`;
  }, [
    busyStep,
    payAsset,
    evmAddress,
    isOnBase,
    amountInRaw,
    insufficientBalance,
    quoting,
    quoteError,
    quote,
    needsApproval,
    receiveAsset,
  ]);

  const primaryDisabled =
    busyStep !== null ||
    (Boolean(evmAddress) &&
      isOnBase &&
      (!amountInRaw || amountInRaw <= 0n || insufficientBalance || quoting || !quote));

  const presets =
    payAsset === 'ETH'
      ? ['0.0005', '0.001', '0.002']
      : payAsset === 'USDC'
        ? ['1', '2', '5']
        : null;

  return (
    <div className="surface-card rounded-2xl p-4 sm:p-5">
      {/* Top Bar */}
      <div className="flex items-center justify-between gap-2">
        <div className="flex items-center gap-2">
          <span className="h-2 w-2 rounded-full bg-emerald-400" />
          <span className="text-[13px] font-semibold tracking-tight text-zinc-100">
            Instant Swap
          </span>
          <span className="rounded-full border border-white/[0.08] bg-white/[0.03] px-2 py-0.5 font-mono text-[10px] text-zinc-400">
            Base · Uniswap V3
          </span>
        </div>

        <div className="flex items-center gap-1.5">
          {/* Quick ETH / USDC toggle for the counter-asset */}
          <div className="inline-flex rounded-lg border border-white/[0.08] bg-[#0a0a0d] p-0.5">
            {(['ETH', 'USDC'] as const).map((asset) => {
              const active = payAsset === asset || receiveAsset === asset;
              return (
                <button
                  key={asset}
                  type="button"
                  onClick={() => handleSelectCounterAsset(asset)}
                  className={`rounded-md px-2 py-0.5 font-mono text-[10px] font-medium transition cursor-pointer ${
                    active ? 'bg-white/[0.12] text-zinc-100' : 'text-zinc-500 hover:text-zinc-300'
                  }`}
                >
                  {asset}
                </button>
              );
            })}
          </div>
          <button
            type="button"
            onClick={() => setSettingsOpen((v) => !v)}
            title="Slippage settings"
            className={`rounded-lg border p-1.5 transition cursor-pointer ${
              settingsOpen
                ? 'border-white/25 bg-white/[0.08] text-zinc-100'
                : 'border-white/[0.08] text-zinc-400 hover:text-zinc-200'
            }`}
          >
            <Settings2 className="h-3.5 w-3.5" />
          </button>
        </div>
      </div>

      {/* Collapsible Slippage Drawer */}
      {settingsOpen && (
        <div className="mt-3 flex flex-wrap items-center justify-between gap-2 rounded-xl border border-white/[0.08] bg-[#0a0a0d] px-3 py-2 text-[11px]">
          <span className="text-zinc-400">Max slippage</span>
          <div className="flex items-center gap-1">
            {[
              { label: '0.5%', bps: 50 },
              { label: '1%', bps: 100 },
              { label: '2%', bps: 200 },
              { label: '5%', bps: 500 },
            ].map((opt) => (
              <button
                key={opt.bps}
                type="button"
                onClick={() => setSlippageBps(opt.bps)}
                className={`rounded-md px-2 py-0.5 font-mono text-[11px] transition cursor-pointer ${
                  slippageBps === opt.bps
                    ? 'bg-zinc-100 font-semibold text-zinc-950'
                    : 'bg-white/[0.04] text-zinc-400 hover:text-zinc-100'
                }`}
              >
                {opt.label}
              </button>
            ))}
          </div>
        </div>
      )}

      {/* Pay Box */}
      <div className="mt-3.5 rounded-xl border border-white/[0.08] bg-[#0a0a0d] p-3.5 transition focus-within:border-white/[0.2]">
        <div className="flex items-center justify-between text-[11px] text-zinc-500">
          <span>You pay</span>
          {evmAddress ? (
            <button
              type="button"
              onClick={handleSetMax}
              className="font-mono text-[11px] text-zinc-400 transition hover:text-zinc-100 cursor-pointer"
            >
              Bal: {formatCompactToken(payBalanceRaw, DECIMALS[payAsset])} {payAsset}
            </button>
          ) : (
            <button
              type="button"
              onClick={openModal}
              className="text-[11px] text-zinc-400 hover:text-zinc-100 cursor-pointer"
            >
              Connect wallet
            </button>
          )}
        </div>

        <div className="mt-2 flex items-center justify-between gap-3">
          <input
            type="text"
            inputMode="decimal"
            value={amountInput}
            onChange={(e) => setAmountInput(e.target.value)}
            placeholder="0.0"
            aria-label={`Amount of ${payAsset} to pay`}
            className="w-full min-w-0 bg-transparent font-mono text-2xl font-medium tracking-tight text-zinc-50 placeholder:text-zinc-700 focus:outline-none tabular-nums"
          />
          <span className="shrink-0 rounded-full border border-white/[0.12] bg-white/[0.05] px-3 py-1.5 font-mono text-xs font-semibold text-zinc-100">
            {payAsset}
          </span>
        </div>

        {/* One-Tap Amount Presets */}
        <div className="mt-2.5 flex flex-wrap items-center gap-1.5">
          {presets ? (
            presets.map((val) => (
              <button
                key={val}
                type="button"
                onClick={() => setAmountInput(val)}
                className={`rounded-md border px-2 py-0.5 font-mono text-[10px] transition cursor-pointer ${
                  amountInput === val
                    ? 'border-white/30 bg-white/[0.1] text-zinc-100'
                    : 'border-white/[0.07] bg-white/[0.02] text-zinc-400 hover:border-white/[0.18] hover:text-zinc-200'
                }`}
              >
                {val} {payAsset}
              </button>
            ))
          ) : (
            <>
              {[25, 50, 100].map((pct) => (
                <button
                  key={pct}
                  type="button"
                  onClick={() => handlePresetPercent(pct)}
                  disabled={!evmAddress || payBalanceRaw <= 0n}
                  className="rounded-md border border-white/[0.07] bg-white/[0.02] px-2 py-0.5 font-mono text-[10px] text-zinc-400 transition hover:border-white/[0.18] hover:text-zinc-200 cursor-pointer disabled:opacity-40"
                >
                  {pct === 100 ? 'MAX' : `${pct}%`}
                </button>
              ))}
            </>
          )}
          {evmAddress && payBalanceRaw > 0n && presets && (
            <button
              type="button"
              onClick={handleSetMax}
              className="rounded-md border border-white/[0.07] bg-white/[0.02] px-2 py-0.5 font-mono text-[10px] text-emerald-300/90 transition hover:border-emerald-500/40 cursor-pointer"
            >
              MAX
            </button>
          )}
        </div>
      </div>

      {/* Flip Direction Arrow */}
      <div className="relative z-10 -my-2.5 flex justify-center">
        <button
          type="button"
          onClick={handleFlip}
          title="Reverse swap direction"
          className="flex h-8 w-8 items-center justify-center rounded-full border border-white/[0.14] bg-[#141418] text-zinc-300 shadow-lg transition hover:border-white/35 hover:text-white cursor-pointer"
        >
          <ArrowDownUp className="h-3.5 w-3.5" />
        </button>
      </div>

      {/* Receive Box */}
      <div className="rounded-xl border border-white/[0.08] bg-[#0a0a0d] p-3.5">
        <div className="flex items-center justify-between text-[11px] text-zinc-500">
          <span>You receive (live quote)</span>
          {evmAddress && (
            <span className="font-mono text-[11px] text-zinc-500">
              Bal: {formatCompactToken(receiveBalanceRaw, DECIMALS[receiveAsset])} {receiveAsset}
            </span>
          )}
        </div>
        <div className="mt-2 flex items-center justify-between gap-3">
          <div className="min-w-0 flex-1 font-mono text-2xl font-medium tracking-tight tabular-nums">
            {quoting ? (
              <span className="inline-flex items-center gap-2 text-base text-zinc-500">
                <Loader2 className="h-4 w-4 animate-spin" />
                <span>Quoting…</span>
              </span>
            ) : formattedOut ? (
              <span className="text-zinc-100">{formattedOut}</span>
            ) : (
              <span className="text-zinc-700">0.0</span>
            )}
          </div>
          <span className="shrink-0 rounded-full border border-emerald-500/30 bg-emerald-500/10 px-3 py-1.5 font-mono text-xs font-semibold text-emerald-200">
            {receiveAsset}
          </span>
        </div>
      </div>

      {/* Quote Details / Price Impact */}
      {quote && !quoting && (
        <div className="mt-3 space-y-1.5 rounded-xl border border-white/[0.06] bg-[#0a0a0d]/70 px-3.5 py-2.5 text-[11px] text-zinc-400">
          <div className="flex items-center justify-between">
            <span>Route</span>
            <span className="font-mono text-zinc-300">{quote.routeLabel}</span>
          </div>
          {minReceived !== null && (
            <div className="flex items-center justify-between">
              <span>Min received ({(slippageBps / 100).toFixed(1)}% slip)</span>
              <span className="font-mono text-zinc-200 tabular-nums">
                {formatCompactToken(minReceived, DECIMALS[receiveAsset])} {receiveAsset}
              </span>
            </div>
          )}
          {quote.priceImpactPct !== null && (
            <div className="flex items-center justify-between">
              <span>Price impact</span>
              <span
                className={`font-mono tabular-nums ${
                  highImpact ? 'text-amber-300' : 'text-emerald-300'
                }`}
              >
                ~{quote.priceImpactPct.toFixed(2)}%
              </span>
            </div>
          )}
          {!compact && pool && (
            <div className="flex items-center justify-between border-t border-white/[0.05] pt-1.5 text-zinc-500">
              <span>Live Base Pool (LP burned)</span>
              <span className="font-mono tabular-nums">
                {formatUnits(pool.wethWei, 18, 4)} WETH · {formatCompactToken(pool.wloveWei, 18)} wLOVE
              </span>
            </div>
          )}
        </div>
      )}

      {highImpact && (
        <div className="mt-2.5 flex items-start gap-2 rounded-xl border border-amber-500/25 bg-amber-500/10 px-3 py-2 text-[11px] text-amber-200">
          <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" />
          <span>
            Early genesis pool (~{pool ? formatUnits(pool.wethWei, 18, 4) : '0.006'} WETH depth). Smaller swaps give much better execution.
          </span>
        </div>
      )}

      {quoteError && (
        <div className="mt-2.5 rounded-xl border border-red-500/25 bg-red-500/10 px-3 py-2 text-[11px] text-red-200">
          {quoteError}
        </div>
      )}

      {/* Single Primary CTA */}
      <button
        type="button"
        disabled={primaryDisabled}
        onClick={() => void handlePrimaryAction()}
        className={`mt-3.5 flex w-full items-center justify-center gap-2 rounded-xl px-4 py-3 text-[13px] font-semibold transition cursor-pointer disabled:cursor-not-allowed disabled:opacity-45 ${
          !evmAddress || !isOnBase
            ? 'bg-zinc-50 text-zinc-950 hover:bg-white'
            : 'bg-emerald-400 text-zinc-950 hover:bg-emerald-300'
        }`}
      >
        {busyStep !== null ? (
          <Loader2 className="h-4 w-4 animate-spin" />
        ) : !evmAddress ? (
          <Wallet className="h-4 w-4" />
        ) : null}
        <span>{primaryLabel}</span>
      </button>

      {/* Live Transaction Receipt + 1-Click Add Token */}
      {lastTxHash && (
        <div className="mt-3 flex flex-wrap items-center justify-between gap-2 rounded-xl border border-emerald-500/30 bg-emerald-500/10 px-3.5 py-2.5 text-xs text-emerald-200">
          <div className="flex items-center gap-2">
            {lastTxStatus === 'pending' ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <CheckCircle2 className="h-3.5 w-3.5 text-emerald-400" />
            )}
            <span>
              {lastTxStatus === 'confirmed'
                ? 'Confirmed on Base!'
                : lastTxStatus === 'failed'
                  ? 'Reverted on-chain'
                  : 'Confirming on Base…'}
            </span>
          </div>
          <div className="flex items-center gap-2.5">
            <a
              href={`https://basescan.org/tx/${lastTxHash}`}
              target="_blank"
              rel="noreferrer"
              className="inline-flex items-center gap-1 font-mono text-[11px] underline decoration-emerald-400/50 underline-offset-2 hover:text-white"
            >
              <span>BaseScan</span>
              <ArrowUpRight className="h-3 w-3" />
            </a>
            <button
              type="button"
              onClick={() => void addWloveToWallet()}
              className="inline-flex items-center gap-1 rounded-md border border-emerald-400/40 bg-emerald-400/15 px-2 py-0.5 text-[11px] font-medium text-emerald-100 hover:bg-emerald-400/25 cursor-pointer"
            >
              <Plus className="h-3 w-3" />
              <span>Add wLOVE</span>
            </button>
          </div>
        </div>
      )}

      {/* Clean Micro-Footer */}
      <div className="mt-3 flex flex-wrap items-center justify-between gap-2 pt-1 text-[11px] text-zinc-500">
        <div className="flex items-center gap-3">
          <a
            href={`https://dexscreener.com/base/${BASE_WLOVE_POOL}`}
            target="_blank"
            rel="noreferrer"
            className="inline-flex items-center gap-0.5 hover:text-zinc-200"
          >
            <span>Chart</span>
            <ArrowUpRight className="h-3 w-3" />
          </a>
          <a
            href={`https://app.uniswap.org/swap?chain=base&outputCurrency=${BASE_WLOVE_ADDRESS}`}
            target="_blank"
            rel="noreferrer"
            className="inline-flex items-center gap-0.5 hover:text-zinc-200"
          >
            <span>Uniswap</span>
            <ArrowUpRight className="h-3 w-3" />
          </a>
          {evmAddress && (
            <button
              type="button"
              onClick={() => void addWloveToWallet()}
              className="inline-flex items-center gap-0.5 hover:text-zinc-200 cursor-pointer"
            >
              <span>+ Add wLOVE</span>
            </button>
          )}
        </div>
        {onOpenBridge && (
          <button
            type="button"
            onClick={onOpenBridge}
            className="text-zinc-400 hover:text-zinc-100 cursor-pointer"
          >
            Bridge to native L1 →
          </button>
        )}
      </div>
    </div>
  );
}
