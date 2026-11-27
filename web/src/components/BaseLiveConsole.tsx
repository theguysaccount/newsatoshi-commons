import { useCallback, useEffect, useState } from 'react';
import {
  ArrowUpRight,
  CheckCircle2,
  Coins,
  Download,
  ExternalLink,
  KeyRound,
  Layers,
  Loader2,
  RefreshCw,
  Rocket,
  Search,
  Sparkles,
  Wallet,
  Zap,
} from 'lucide-react';
import { useWallet } from '../wallet/WalletContext';
import {
  BASE_FAIR_CURVE_FACTORY_ADDRESS,
  BASE_PAYWORD_CHANNEL_ADDRESS,
  BASE_USDC_ADDRESS,
  ZERO_ADDRESS,
  encodeApprove,
  encodeCreateFairCurve,
  encodeFairCurveBuy,
  encodeFairCurveClaimRevenue,
  encodeFairCurveInjectRevenue,
  encodeFairCurveSell,
  encodePayWordClaim,
  encodePayWordCloseAndRefund,
  encodePayWordOpenChannel,
  formatUnits,
  generateBrowserPayWordChain,
  parseUnits,
  readErc20Allowance,
  readErc20Balance,
  readFairCurveClaimable,
  readPayWordChannelId,
} from '../wallet/evm';
import { CopyButton, truncateHash } from './ui/CopyButton';

interface LiveCurveEntry {
  pool_index: number;
  pool_address: string;
  token_address: string;
  name: string;
  symbol: string;
  reserve_token: string;
  reserve_symbol: string;
  fee_sink: string;
  holder_dividend_fee_mode: boolean;
  swap_fee_bps: number;
  virtual_reserve_usdc: number;
  virtual_supply_tokens: number;
  curve_reserve_usdc: number;
  total_supply_tokens: number;
  spot_price_usdc: number;
  total_revenue_injected_usdc: number;
  total_revenue_claimed_usdc: number;
  unclaimed_revenue_usdc: number;
  metadata_uri: string;
  basescan_pool_url: string;
  basescan_token_url: string;
}

interface LiveCurvesResponse {
  ok: boolean;
  factory_address?: string;
  total_pool_count?: number;
  curves?: LiveCurveEntry[];
  error?: string;
}

interface LiveChannelResponse {
  ok: boolean;
  found?: boolean;
  channel_id?: string;
  payer?: string;
  recipient?: string;
  token?: string;
  token_symbol?: string;
  initial_root?: string;
  current_root?: string;
  unit_amount_raw?: string;
  unit_amount_usdc?: number | null;
  max_steps?: number;
  claimed_steps?: number;
  remaining_steps?: number;
  claimed_amount_usdc?: number | null;
  remaining_amount_usdc?: number | null;
  expiry_unix?: number;
  closed?: boolean;
  reason?: string;
  error?: string;
}

interface HolderPoolState {
  tokenBalanceWei: bigint;
  claimableUsdcRaw: bigint;
}

interface BaseLiveConsoleProps {
  onToast: (kind: 'success' | 'error', message: string) => void;
}

export function BaseLiveConsole({ onToast }: BaseLiveConsoleProps) {
  const {
    evmAddress,
    evmChainId,
    balances,
    openModal,
    switchEvmChain,
    sendEvmTx,
    waitForEvmReceipt,
    refreshBalances,
  } = useWallet();

  // Live Base Mainnet curves directory
  const [curvesData, setCurvesData] = useState<LiveCurvesResponse | null>(null);
  const [curvesLoading, setCurvesLoading] = useState<boolean>(false);
  const [selectedPoolAddr, setSelectedPoolAddr] = useState<string>('');
  const [holderState, setHolderState] = useState<HolderPoolState | null>(null);

  // Per-pool action inputs
  const [buyUsdcStr, setBuyUsdcStr] = useState<string>('1');
  const [sellTokensStr, setSellTokensStr] = useState<string>('1000');
  const [injectUsdcStr, setInjectUsdcStr] = useState<string>('0.50');
  const [poolTxBusy, setPoolTxBusy] = useState<string | null>(null);
  const [lastTxHash, setLastTxHash] = useState<string | null>(null);

  // 1-Click Launch New Fair Curve form
  const [launchName, setLaunchName] = useState<string>('Agent Inference Pool');
  const [launchSym, setLaunchSym] = useState<string>('AGINF');
  const [launchVReserve, setLaunchVReserve] = useState<string>('10000');
  const [launchVSupply, setLaunchVSupply] = useState<string>('1000000000');
  const [launchFeeBps, setLaunchFeeBps] = useState<string>('100');
  const [launchFeeMode, setLaunchFeeMode] = useState<'holders' | 'treasury'>('holders');
  const [launchFeeSink, setLaunchFeeSink] = useState<string>('');
  const [launchUri, setLaunchUri] = useState<string>('https://newsatoshi.org/launchpad/');
  const [launchBusy, setLaunchBusy] = useState<boolean>(false);

  // Live PayWords Channel Lab (WebCrypto + EIP-1193 Base Mainnet)
  const [pwRecipient, setPwRecipient] = useState<string>(
    '0x1744535521EB420be9700475400455a4Ad0EBEa2'
  );
  const [pwUnitUsdc, setPwUnitUsdc] = useState<string>('0.002');
  const [pwSteps, setPwSteps] = useState<string>('100');
  const [pwExpiryHours, setPwExpiryHours] = useState<string>('24');
  const [pwLocalChain, setPwLocalChain] = useState<{
    seedHex: string;
    rootHex: string;
    chain: string[];
    maxSteps: number;
    spentSteps: number;
    expiryUnix: number;
    predictedChannelId: string | null;
  } | null>(null);
  const [pwBusy, setPwBusy] = useState<string | null>(null);

  // Live Channel Inspector (read-only Base RPC)
  const [inspectId, setInspectId] = useState<string>('');
  const [inspectBusy, setInspectBusy] = useState<boolean>(false);
  const [inspectRes, setInspectRes] = useState<LiveChannelResponse | null>(null);
  const [claimPreimageInput, setClaimPreimageInput] = useState<string>('');
  const [claimStepsInput, setClaimStepsInput] = useState<string>('1');

  const fetchCurves = useCallback(async (forceRefresh = false) => {
    setCurvesLoading(true);
    try {
      const resp = await fetch(`/api/v2/base/curves${forceRefresh ? '?refresh=1' : ''}`, {
        headers: { Accept: 'application/json' },
      });
      const json = (await resp.json()) as LiveCurvesResponse;
      setCurvesData(json);
      if (json.ok && json.curves && json.curves.length > 0) {
        setSelectedPoolAddr((prev) => prev || json.curves![0].pool_address);
      }
    } catch (err) {
      setCurvesData({ ok: false, error: err instanceof Error ? err.message : 'Failed to load curves' });
    } finally {
      setCurvesLoading(false);
    }
  }, []);

  useEffect(() => {
    void fetchCurves(false);
  }, [fetchCurves]);

  const selectedPool =
    curvesData?.curves?.find((c) => c.pool_address.toLowerCase() === selectedPoolAddr.toLowerCase()) ||
    curvesData?.curves?.[0] ||
    null;

  const refreshHolderState = useCallback(async () => {
    if (!evmAddress || !selectedPool) {
      setHolderState(null);
      return;
    }
    try {
      const [bal, claimable] = await Promise.all([
        readErc20Balance('base', selectedPool.token_address, evmAddress),
        readFairCurveClaimable(selectedPool.pool_address, evmAddress),
      ]);
      setHolderState({ tokenBalanceWei: bal, claimableUsdcRaw: claimable });
    } catch {
      setHolderState(null);
    }
  }, [evmAddress, selectedPool]);

  useEffect(() => {
    void refreshHolderState();
  }, [refreshHolderState]);

  const ensureBaseWallet = useCallback(async (): Promise<boolean> => {
    if (!evmAddress) {
      openModal();
      return false;
    }
    if (evmChainId !== 8453) {
      const switched = await switchEvmChain('base');
      if (!switched) {
        onToast('error', 'Please switch your wallet to Base Mainnet (Chain ID 8453).');
        return false;
      }
    }
    return true;
  }, [evmAddress, evmChainId, openModal, switchEvmChain, onToast]);

  const ensureUsdcApproval = useCallback(
    async (spender: string, neededRaw: bigint) => {
      if (!evmAddress) throw new Error('Connect an EVM wallet first');
      const current = await readErc20Allowance('base', BASE_USDC_ADDRESS, evmAddress, spender);
      if (current >= neededRaw) return;
      onToast('success', 'Approving exact USDC allowance on Base Mainnet...');
      const approveTx = await sendEvmTx({
        to: BASE_USDC_ADDRESS,
        data: encodeApprove(spender, neededRaw),
      });
      const status = await waitForEvmReceipt('base', approveTx);
      if (status !== 'success') {
        throw new Error(`USDC approve transaction ${status}`);
      }
    },
    [evmAddress, onToast, sendEvmTx, waitForEvmReceipt]
  );

  const handleBuyPool = async () => {
    if (!selectedPool) return;
    if (!(await ensureBaseWallet())) return;
    const raw = parseUnits(buyUsdcStr, 6);
    if (!raw || raw <= 0n) {
      onToast('error', 'Enter a valid USDC amount (> 0).');
      return;
    }
    setPoolTxBusy('buy');
    try {
      await ensureUsdcApproval(selectedPool.pool_address, raw);
      const txHash = await sendEvmTx({
        to: selectedPool.pool_address,
        data: encodeFairCurveBuy(raw, 0n),
      });
      setLastTxHash(txHash);
      const status = await waitForEvmReceipt('base', txHash);
      if (status === 'success') {
        onToast('success', `Bought ${selectedPool.symbol} on Base Mainnet (${truncateHash(txHash, 8, 6)})`);
        await Promise.all([fetchCurves(true), refreshHolderState(), refreshBalances()]);
      } else {
        onToast('error', `Buy transaction ${status}. Check BaseScan.`);
      }
    } catch (err) {
      onToast('error', err instanceof Error ? err.message : 'Buy failed');
    } finally {
      setPoolTxBusy(null);
    }
  };

  const handleSellPool = async () => {
    if (!selectedPool) return;
    if (!(await ensureBaseWallet())) return;
    const wei = parseUnits(sellTokensStr, 18);
    if (!wei || wei <= 0n) {
      onToast('error', `Enter a valid ${selectedPool.symbol} token amount.`);
      return;
    }
    setPoolTxBusy('sell');
    try {
      const txHash = await sendEvmTx({
        to: selectedPool.pool_address,
        data: encodeFairCurveSell(wei, 0n),
      });
      setLastTxHash(txHash);
      const status = await waitForEvmReceipt('base', txHash);
      if (status === 'success') {
        onToast('success', `Sold ${selectedPool.symbol} back into USDC curve reserve!`);
        await Promise.all([fetchCurves(true), refreshHolderState(), refreshBalances()]);
      } else {
        onToast('error', `Sell transaction ${status}.`);
      }
    } catch (err) {
      onToast('error', err instanceof Error ? err.message : 'Sell failed');
    } finally {
      setPoolTxBusy(null);
    }
  };

  const handleInjectRevenue = async () => {
    if (!selectedPool) return;
    if (!(await ensureBaseWallet())) return;
    const raw = parseUnits(injectUsdcStr, 6);
    if (!raw || raw <= 0n) {
      onToast('error', 'Enter a valid USDC revenue amount to inject.');
      return;
    }
    setPoolTxBusy('inject');
    try {
      await ensureUsdcApproval(selectedPool.pool_address, raw);
      const txHash = await sendEvmTx({
        to: selectedPool.pool_address,
        data: encodeFairCurveInjectRevenue(raw),
      });
      setLastTxHash(txHash);
      const status = await waitForEvmReceipt('base', txHash);
      if (status === 'success') {
        onToast('success', `Injected $${injectUsdcStr} USDC revenue directly to ${selectedPool.symbol} holders!`);
        await Promise.all([fetchCurves(true), refreshHolderState(), refreshBalances()]);
      } else {
        onToast('error', `injectRevenue transaction ${status}.`);
      }
    } catch (err) {
      onToast('error', err instanceof Error ? err.message : 'Revenue injection failed');
    } finally {
      setPoolTxBusy(null);
    }
  };

  const handleClaimDividends = async () => {
    if (!selectedPool) return;
    if (!(await ensureBaseWallet())) return;
    setPoolTxBusy('claim');
    try {
      const txHash = await sendEvmTx({
        to: selectedPool.pool_address,
        data: encodeFairCurveClaimRevenue(),
      });
      setLastTxHash(txHash);
      const status = await waitForEvmReceipt('base', txHash);
      if (status === 'success') {
        onToast('success', `Claimed USDC PayWords dividends from ${selectedPool.symbol}!`);
        await Promise.all([fetchCurves(true), refreshHolderState(), refreshBalances()]);
      } else {
        onToast('error', `claimRevenue transaction ${status}.`);
      }
    } catch (err) {
      onToast('error', err instanceof Error ? err.message : 'Claim failed');
    } finally {
      setPoolTxBusy(null);
    }
  };

  const handleLaunchCurve = async () => {
    if (!(await ensureBaseWallet())) return;
    const vRaw = parseUnits(launchVReserve, 6);
    const y0Wei = parseUnits(launchVSupply, 18);
    const feeBps = Math.max(0, Math.min(500, Math.round(Number(launchFeeBps) || 0)));
    if (!launchName.trim() || !launchSym.trim() || !vRaw || !y0Wei) {
      onToast('error', 'Please fill valid name, symbol, virtual reserve, and supply cap.');
      return;
    }
    const feeSink =
      launchFeeMode === 'holders'
        ? ZERO_ADDRESS
        : /^0x[0-9a-fA-F]{40}$/.test(launchFeeSink.trim())
          ? launchFeeSink.trim()
          : evmAddress!;

    setLaunchBusy(true);
    try {
      const data = encodeCreateFairCurve({
        name: launchName.trim(),
        symbol: launchSym.trim().toUpperCase(),
        reserveToken: BASE_USDC_ADDRESS,
        virtualReserveRaw: vRaw,
        virtualSupplyWei: y0Wei,
        swapFeeBps: feeBps,
        feeSink,
        metadataUri: launchUri.trim() || 'https://newsatoshi.org/launchpad/',
      });
      const txHash = await sendEvmTx({ to: BASE_FAIR_CURVE_FACTORY_ADDRESS, data });
      setLastTxHash(txHash);
      const status = await waitForEvmReceipt('base', txHash);
      if (status === 'success') {
        onToast('success', `Launched 0%-premine Fair Curve ${launchSym.toUpperCase()} on Base Mainnet!`);
        await fetchCurves(true);
      } else {
        onToast('error', `createCurve transaction ${status}.`);
      }
    } catch (err) {
      onToast('error', err instanceof Error ? err.message : 'Launch failed');
    } finally {
      setLaunchBusy(false);
    }
  };

  const handleGenerateLocalChain = async () => {
    const steps = Math.max(1, Math.min(2048, Math.floor(Number(pwSteps) || 100)));
    const hours = Math.max(1, Math.min(2160, Number(pwExpiryHours) || 24));
    const expiryUnix = Math.floor(Date.now() / 1000) + Math.round(hours * 3600);
    setPwBusy('gen');
    try {
      const generated = await generateBrowserPayWordChain(steps);
      let predictedChannelId: string | null = null;
      const unitRaw = parseUnits(pwUnitUsdc, 6);
      if (evmAddress && unitRaw && /^0x[0-9a-fA-F]{40}$/.test(pwRecipient.trim())) {
        try {
          predictedChannelId = await readPayWordChannelId({
            payer: evmAddress,
            recipient: pwRecipient.trim(),
            token: BASE_USDC_ADDRESS,
            rootHex: generated.rootHex,
            unitAmountRaw: unitRaw,
            maxSteps: steps,
            expiryUnix,
          });
        } catch {
          predictedChannelId = null;
        }
      }
      setPwLocalChain({
        seedHex: generated.seedHex,
        rootHex: generated.rootHex,
        chain: generated.chain,
        maxSteps: steps,
        spentSteps: 0,
        expiryUnix,
        predictedChannelId,
      });
      if (predictedChannelId) setInspectId(predictedChannelId);
      onToast('success', `Generated ${steps}-step SHA-256 PayWord chain locally in WebCrypto.`);
    } catch (err) {
      onToast('error', err instanceof Error ? err.message : 'Chain generation failed');
    } finally {
      setPwBusy(null);
    }
  };

  const handleOpenPayWordChannel = async () => {
    if (!pwLocalChain) {
      onToast('error', 'Generate a local SHA-256 PayWord chain first.');
      return;
    }
    if (!(await ensureBaseWallet())) return;
    if (!/^0x[0-9a-fA-F]{40}$/.test(pwRecipient.trim())) {
      onToast('error', 'Enter a valid 0x recipient address.');
      return;
    }
    const unitRaw = parseUnits(pwUnitUsdc, 6);
    if (!unitRaw || unitRaw <= 0n) {
      onToast('error', 'Enter a valid USDC price per call.');
      return;
    }
    const totalDeposit = unitRaw * BigInt(pwLocalChain.maxSteps);
    setPwBusy('open');
    try {
      const cid = await readPayWordChannelId({
        payer: evmAddress!,
        recipient: pwRecipient.trim(),
        token: BASE_USDC_ADDRESS,
        rootHex: pwLocalChain.rootHex,
        unitAmountRaw: unitRaw,
        maxSteps: pwLocalChain.maxSteps,
        expiryUnix: pwLocalChain.expiryUnix,
      });
      setPwLocalChain((prev) => (prev ? { ...prev, predictedChannelId: cid } : prev));
      setInspectId(cid);

      await ensureUsdcApproval(BASE_PAYWORD_CHANNEL_ADDRESS, totalDeposit);
      const data = encodePayWordOpenChannel({
        recipient: pwRecipient.trim(),
        token: BASE_USDC_ADDRESS,
        rootHex: pwLocalChain.rootHex,
        unitAmountRaw: unitRaw,
        maxSteps: pwLocalChain.maxSteps,
        expiryUnix: pwLocalChain.expiryUnix,
      });
      const txHash = await sendEvmTx({ to: BASE_PAYWORD_CHANNEL_ADDRESS, data });
      setLastTxHash(txHash);
      const status = await waitForEvmReceipt('base', txHash);
      if (status === 'success') {
        onToast('success', `Opened live Base USDC PayWord channel (${truncateHash(cid, 8, 6)})!`);
        await Promise.all([refreshBalances(), handleInspectChannel(cid)]);
      } else {
        onToast('error', `openChannel transaction ${status}.`);
      }
    } catch (err) {
      onToast('error', err instanceof Error ? err.message : 'openChannel failed');
    } finally {
      setPwBusy(null);
    }
  };

  const handleStepVoucher = () => {
    if (!pwLocalChain) return;
    if (pwLocalChain.spentSteps >= pwLocalChain.maxSteps) {
      onToast('error', 'Channel exhausted — generate a fresh chain.');
      return;
    }
    const nextSteps = pwLocalChain.spentSteps + 1;
    const preimage = pwLocalChain.chain[pwLocalChain.maxSteps - nextSteps];
    setPwLocalChain({ ...pwLocalChain, spentSteps: nextSteps });
    setClaimPreimageInput(preimage);
    setClaimStepsInput(String(nextSteps));
  };

  const handleInspectChannel = useCallback(
    async (explicitId?: string) => {
      const target = (explicitId ?? inspectId).trim();
      if (!target) return;
      setInspectBusy(true);
      try {
        const resp = await fetch(`/api/v2/base/channel?id=${encodeURIComponent(target)}`, {
          headers: { Accept: 'application/json' },
        });
        const json = (await resp.json()) as LiveChannelResponse;
        setInspectRes(json);
      } catch (err) {
        setInspectRes({
          ok: false,
          error: err instanceof Error ? err.message : 'RPC inspection failed',
        });
      } finally {
        setInspectBusy(false);
      }
    },
    [inspectId]
  );

  const handleClaimChannel = async () => {
    const cid = inspectId.trim();
    const steps = Math.max(1, Math.min(4096, Math.floor(Number(claimStepsInput) || 1)));
    const pre = claimPreimageInput.trim();
    if (!cid || !pre) {
      onToast('error', 'Provide channelId, steps, and preimage w_k to settle on-chain.');
      return;
    }
    if (!(await ensureBaseWallet())) return;
    setPwBusy('claim');
    try {
      const txHash = await sendEvmTx({
        to: BASE_PAYWORD_CHANNEL_ADDRESS,
        data: encodePayWordClaim(cid, steps, pre),
      });
      setLastTxHash(txHash);
      const status = await waitForEvmReceipt('base', txHash);
      if (status === 'success') {
        onToast('success', `Settled ${steps} PayWord step(s) on Base Mainnet (0% protocol cut)!`);
        await Promise.all([handleInspectChannel(cid), refreshBalances()]);
      } else {
        onToast('error', `claim() transaction ${status}.`);
      }
    } catch (err) {
      onToast('error', err instanceof Error ? err.message : 'claim() failed');
    } finally {
      setPwBusy(null);
    }
  };

  const handleCloseChannel = async () => {
    const cid = inspectId.trim();
    if (!cid) return;
    if (!(await ensureBaseWallet())) return;
    setPwBusy('close');
    try {
      const txHash = await sendEvmTx({
        to: BASE_PAYWORD_CHANNEL_ADDRESS,
        data: encodePayWordCloseAndRefund(cid),
      });
      setLastTxHash(txHash);
      const status = await waitForEvmReceipt('base', txHash);
      if (status === 'success') {
        onToast('success', 'Channel cooperatively closed & unused USDC refunded to payer!');
        await Promise.all([handleInspectChannel(cid), refreshBalances()]);
      } else {
        onToast('error', `closeAndRefund() ${status} (must be called by channel recipient).`);
      }
    } catch (err) {
      onToast('error', err instanceof Error ? err.message : 'closeAndRefund() failed');
    } finally {
      setPwBusy(null);
    }
  };

  const latestLocalPreimage =
    pwLocalChain && pwLocalChain.spentSteps > 0
      ? pwLocalChain.chain[pwLocalChain.maxSteps - pwLocalChain.spentSteps]
      : null;

  return (
    <section
      id="launchpad"
      className="border-t border-white/[0.08] pt-14 space-y-8"
      aria-label="Live Base Mainnet Fair Curve Launchpad and PayWord Channel Console"
    >
      {/* Header + Wallet Connection Bar */}
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <p className="font-mono text-[11px] uppercase tracking-[0.14em] text-amber-300">
            Live On-Chain Base Mainnet Console · 0% Protocol Fee · Non-Custodial Wallet
          </p>
          <h2 className="font-display mt-1.5 text-3xl tracking-tight text-zinc-100 sm:text-4xl">
            Trade, Launch &amp; Settle Directly on Base Mainnet.
          </h2>
          <p className="mt-2 max-w-2xl text-[13px] leading-relaxed text-zinc-400">
            Zero server keys, zero fake data. Reads live state from Sourcify-verified{' '}
            <code className="font-mono text-xs text-zinc-200">NewsatoshiFairCurveFactory</code> and{' '}
            <code className="font-mono text-xs text-zinc-200">NewsatoshiPayWordChannel</code> and signs
            transactions directly through your MetaMask, Coinbase Wallet, or Rabby extension.
          </p>
        </div>

        <div className="flex flex-wrap items-center gap-2.5">
          {evmAddress ? (
            <div className="inline-flex items-center gap-2.5 rounded-full border border-emerald-500/30 bg-emerald-500/10 px-3.5 py-1.5 font-mono text-xs text-emerald-200">
              <CheckCircle2 className="h-3.5 w-3.5 text-emerald-400" />
              <span>{truncateHash(evmAddress, 6, 4)}</span>
              <span className="text-zinc-500">|</span>
              <span>${formatUnits(balances.baseUsdcRaw, 6, 2)} USDC</span>
              <span className="text-zinc-500">|</span>
              <span>{evmChainId === 8453 ? 'Base' : `Chain ${evmChainId ?? '?'}`}</span>
            </div>
          ) : (
            <button
              type="button"
              onClick={openModal}
              className="inline-flex items-center gap-2 rounded-full bg-amber-400 px-4 py-2 text-xs font-semibold text-zinc-950 transition hover:bg-amber-300 cursor-pointer"
            >
              <Wallet className="h-3.5 w-3.5" />
              <span>Connect MetaMask / Coinbase / Rabby</span>
            </button>
          )}
          <button
            type="button"
            onClick={() => void fetchCurves(true)}
            disabled={curvesLoading}
            className="inline-flex items-center gap-1.5 rounded-full border border-white/[0.1] bg-white/[0.03] px-3 py-1.5 text-xs text-zinc-300 hover:text-white cursor-pointer disabled:opacity-50"
          >
            <RefreshCw className={`h-3.5 w-3.5 ${curvesLoading ? 'animate-spin' : ''}`} />
            <span>Refresh Base RPC</span>
          </button>
        </div>
      </div>

      {lastTxHash && (
        <div className="flex flex-wrap items-center justify-between gap-2 rounded-xl border border-emerald-500/25 bg-emerald-500/[0.06] px-4 py-2.5 font-mono text-xs text-emerald-200">
          <span>Latest Base Mainnet TX: {truncateHash(lastTxHash, 12, 8)}</span>
          <a
            href={`https://basescan.org/tx/${lastTxHash}`}
            target="_blank"
            rel="noreferrer"
            className="inline-flex items-center gap-1 text-emerald-300 underline hover:text-white"
          >
            <span>Inspect on BaseScan</span>
            <ExternalLink className="h-3 w-3" />
          </a>
        </div>
      )}

      {/* Row 1: Live Fair Curves Directory + 1-Click Trade / Inject / Claim + 1-Click Launch */}
      <div className="grid grid-cols-1 gap-5 lg:grid-cols-12">
        {/* Left 7 cols: Live Deployed Curves & Direct Pool Terminal */}
        <div className="lg:col-span-7 rounded-2xl border border-fuchsia-500/25 bg-fuchsia-500/[0.025] p-5 sm:p-6 space-y-5">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <div className="inline-flex items-center gap-2 font-mono text-[11px] uppercase tracking-[0.1em] text-fuchsia-300">
              <Layers className="h-4 w-4" />
              <span>
                Live Base Mainnet Fair Curves ({curvesData?.total_pool_count ?? 0} Deployed)
              </span>
            </div>
            <a
              href={`https://basescan.org/address/${BASE_FAIR_CURVE_FACTORY_ADDRESS}`}
              target="_blank"
              rel="noreferrer"
              className="inline-flex items-center gap-1 font-mono text-[11px] text-zinc-400 hover:text-white"
            >
              <span>Factory {truncateHash(BASE_FAIR_CURVE_FACTORY_ADDRESS, 6, 4)}</span>
              <ArrowUpRight className="h-3 w-3" />
            </a>
          </div>

          {curvesLoading && !curvesData && (
            <div className="flex items-center gap-2 py-6 font-mono text-xs text-zinc-400">
              <Loader2 className="h-4 w-4 animate-spin" />
              <span>Reading live pools from Base Mainnet JSON-RPC...</span>
            </div>
          )}

          {curvesData && !curvesData.ok && (
            <div className="rounded-xl border border-rose-500/30 bg-rose-500/10 p-3 font-mono text-xs text-rose-200">
              Base RPC read error: {curvesData.error}
            </div>
          )}

          {curvesData?.ok && curvesData.curves && curvesData.curves.length > 0 && (
            <div className="space-y-2.5">
              {curvesData.curves.map((c) => {
                const active = selectedPool?.pool_address.toLowerCase() === c.pool_address.toLowerCase();
                return (
                  <button
                    key={c.pool_address}
                    type="button"
                    onClick={() => setSelectedPoolAddr(c.pool_address)}
                    className={`w-full rounded-xl border p-3.5 text-left transition cursor-pointer ${
                      active
                        ? 'border-fuchsia-400/60 bg-[#0d1018]'
                        : 'border-white/[0.08] bg-[#090c12] hover:border-white/[0.2]'
                    }`}
                  >
                    <div className="flex flex-wrap items-center justify-between gap-2">
                      <div className="flex items-center gap-2">
                        <span className="font-mono text-sm font-semibold text-zinc-100">
                          {c.symbol || 'CURVE'}
                        </span>
                        <span className="text-xs text-zinc-400">{c.name}</span>
                        <span className="rounded-full border border-emerald-500/25 bg-emerald-500/10 px-2 py-0.5 font-mono text-[10px] text-emerald-300">
                          0% Premine · {(c.swap_fee_bps / 100).toFixed(2)}% Fee
                        </span>
                      </div>
                      <span className="font-mono text-xs text-fuchsia-300">
                        Spot ${c.spot_price_usdc.toFixed(6)}
                      </span>
                    </div>
                    <div className="mt-2.5 grid grid-cols-2 gap-2 font-mono text-[11px] sm:grid-cols-4">
                      <div>
                        <span className="block text-[10px] text-zinc-500">Curve Reserve</span>
                        <span className="text-zinc-200">${c.curve_reserve_usdc.toFixed(4)} USDC</span>
                      </div>
                      <div>
                        <span className="block text-[10px] text-zinc-500">PayWords Injected</span>
                        <span className="text-emerald-300">${c.total_revenue_injected_usdc.toFixed(4)}</span>
                      </div>
                      <div>
                        <span className="block text-[10px] text-zinc-500">Circulating Supply</span>
                        <span className="text-zinc-200">{c.total_supply_tokens.toLocaleString()}</span>
                      </div>
                      <div>
                        <span className="block text-[10px] text-zinc-500">Fee Routing</span>
                        <span className="text-cyan-300">
                          {c.holder_dividend_fee_mode ? '100% Holders' : 'Treasury Sink'}
                        </span>
                      </div>
                    </div>
                  </button>
                );
              })}
            </div>
          )}

          {/* Active Pool 1-Click Trade / Inject Revenue / Claim Panel */}
          {selectedPool && (
            <div className="rounded-xl border border-white/[0.09] bg-[#090c12] p-4 space-y-4">
              <div className="flex flex-wrap items-center justify-between gap-2 border-b border-white/[0.07] pb-3">
                <div>
                  <p className="font-mono text-xs font-semibold text-zinc-100">
                    Active Pool: {selectedPool.name} ({selectedPool.symbol})
                  </p>
                  <div className="mt-1 flex flex-wrap items-center gap-2 font-mono text-[11px] text-zinc-400">
                    <span>Pool {truncateHash(selectedPool.pool_address, 8, 6)}</span>
                    <CopyButton value={selectedPool.pool_address} />
                    <a
                      href={selectedPool.basescan_pool_url}
                      target="_blank"
                      rel="noreferrer"
                      className="text-cyan-300 hover:underline"
                    >
                      BaseScan ↗
                    </a>
                  </div>
                </div>
                {holderState && (
                  <div className="text-right font-mono text-[11px]">
                    <div className="text-zinc-300">
                      Your Balance: {formatUnits(holderState.tokenBalanceWei, 18, 2)} {selectedPool.symbol}
                    </div>
                    <div className="text-emerald-300">
                      Claimable Dividends: ${formatUnits(holderState.claimableUsdcRaw, 6, 6)} USDC
                    </div>
                  </div>
                )}
              </div>

              <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
                {/* Buy */}
                <div className="rounded-lg border border-white/[0.07] bg-white/[0.02] p-3 space-y-2">
                  <label className="block font-mono text-[10px] uppercase tracking-wider text-zinc-400">
                    Buy with Base USDC
                  </label>
                  <input
                    type="text"
                    value={buyUsdcStr}
                    onChange={(e) => setBuyUsdcStr(e.target.value)}
                    className="w-full rounded-md border border-white/[0.1] bg-[#05070b] px-2.5 py-1.5 font-mono text-xs text-zinc-100"
                    placeholder="1.00"
                  />
                  <button
                    type="button"
                    onClick={() => void handleBuyPool()}
                    disabled={poolTxBusy !== null}
                    className="w-full inline-flex items-center justify-center gap-1.5 rounded-lg bg-emerald-400 px-3 py-1.5 text-xs font-semibold text-zinc-950 hover:bg-emerald-300 cursor-pointer disabled:opacity-50"
                  >
                    {poolTxBusy === 'buy' ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Coins className="h-3.5 w-3.5" />}
                    <span>Approve &amp; Buy</span>
                  </button>
                </div>

                {/* Sell */}
                <div className="rounded-lg border border-white/[0.07] bg-white/[0.02] p-3 space-y-2">
                  <label className="block font-mono text-[10px] uppercase tracking-wider text-zinc-400">
                    Redeem {selectedPool.symbol} → USDC
                  </label>
                  <input
                    type="text"
                    value={sellTokensStr}
                    onChange={(e) => setSellTokensStr(e.target.value)}
                    className="w-full rounded-md border border-white/[0.1] bg-[#05070b] px-2.5 py-1.5 font-mono text-xs text-zinc-100"
                    placeholder="1000"
                  />
                  <button
                    type="button"
                    onClick={() => void handleSellPool()}
                    disabled={poolTxBusy !== null}
                    className="w-full inline-flex items-center justify-center gap-1.5 rounded-lg border border-white/[0.15] bg-white/[0.06] px-3 py-1.5 text-xs font-semibold text-zinc-100 hover:bg-white/[0.12] cursor-pointer disabled:opacity-50"
                  >
                    {poolTxBusy === 'sell' ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Zap className="h-3.5 w-3.5" />}
                    <span>Sell to Curve</span>
                  </button>
                </div>

                {/* Inject PayWords Revenue + Claim */}
                <div className="rounded-lg border border-white/[0.07] bg-white/[0.02] p-3 space-y-2">
                  <label className="block font-mono text-[10px] uppercase tracking-wider text-zinc-400">
                    Inject API Revenue (USDC)
                  </label>
                  <input
                    type="text"
                    value={injectUsdcStr}
                    onChange={(e) => setInjectUsdcStr(e.target.value)}
                    className="w-full rounded-md border border-white/[0.1] bg-[#05070b] px-2.5 py-1.5 font-mono text-xs text-zinc-100"
                    placeholder="0.50"
                  />
                  <div className="flex gap-1.5">
                    <button
                      type="button"
                      onClick={() => void handleInjectRevenue()}
                      disabled={poolTxBusy !== null}
                      className="flex-1 inline-flex items-center justify-center gap-1 rounded-lg bg-fuchsia-400 px-2 py-1.5 text-[11px] font-semibold text-zinc-950 hover:bg-fuchsia-300 cursor-pointer disabled:opacity-50"
                    >
                      {poolTxBusy === 'inject' ? <Loader2 className="h-3 w-3 animate-spin" /> : <Sparkles className="h-3 w-3" />}
                      <span>Inject</span>
                    </button>
                    <button
                      type="button"
                      onClick={() => void handleClaimDividends()}
                      disabled={poolTxBusy !== null}
                      className="flex-1 inline-flex items-center justify-center gap-1 rounded-lg bg-cyan-400 px-2 py-1.5 text-[11px] font-semibold text-zinc-950 hover:bg-cyan-300 cursor-pointer disabled:opacity-50"
                    >
                      {poolTxBusy === 'claim' ? <Loader2 className="h-3 w-3 animate-spin" /> : <Coins className="h-3 w-3" />}
                      <span>Claim</span>
                    </button>
                  </div>
                </div>
              </div>
            </div>
          )}
        </div>

        {/* Right 5 cols: 1-Click Launch a New 0%-Premine Fair Curve */}
        <div className="lg:col-span-5 rounded-2xl border border-amber-500/25 bg-amber-500/[0.025] p-5 sm:p-6 space-y-4">
          <div className="inline-flex items-center gap-2 font-mono text-[11px] uppercase tracking-[0.1em] text-amber-300">
            <Rocket className="h-4 w-4" />
            <span>Launch 0%-Premine Fair Curve (1 Base TX)</span>
          </div>
          <p className="text-xs leading-relaxed text-zinc-400">
            Deploy an immutable ERC-20 + USDC constant-product pool on Base Mainnet via{' '}
            <code className="font-mono text-zinc-200">NewsatoshiFairCurveFactory</code>. Zero admin
            keys, zero platform cut — creator only pays Base L2 gas (~$0.01).
          </p>

          <div className="grid grid-cols-2 gap-2.5">
            <div>
              <label className="block font-mono text-[10px] uppercase text-zinc-500 mb-1">Token Name</label>
              <input
                type="text"
                value={launchName}
                onChange={(e) => setLaunchName(e.target.value)}
                className="w-full rounded-lg border border-white/[0.1] bg-[#090c12] px-2.5 py-1.5 font-mono text-xs text-zinc-100"
              />
            </div>
            <div>
              <label className="block font-mono text-[10px] uppercase text-zinc-500 mb-1">Ticker</label>
              <input
                type="text"
                value={launchSym}
                onChange={(e) => setLaunchSym(e.target.value)}
                className="w-full rounded-lg border border-white/[0.1] bg-[#090c12] px-2.5 py-1.5 font-mono text-xs text-zinc-100"
              />
            </div>
            <div>
              <label className="block font-mono text-[10px] uppercase text-zinc-500 mb-1">
                Virtual Reserve (USDC)
              </label>
              <input
                type="text"
                value={launchVReserve}
                onChange={(e) => setLaunchVReserve(e.target.value)}
                className="w-full rounded-lg border border-white/[0.1] bg-[#090c12] px-2.5 py-1.5 font-mono text-xs text-zinc-100"
              />
            </div>
            <div>
              <label className="block font-mono text-[10px] uppercase text-zinc-500 mb-1">
                Virtual Supply Cap
              </label>
              <input
                type="text"
                value={launchVSupply}
                onChange={(e) => setLaunchVSupply(e.target.value)}
                className="w-full rounded-lg border border-white/[0.1] bg-[#090c12] px-2.5 py-1.5 font-mono text-xs text-zinc-100"
              />
            </div>
            <div>
              <label className="block font-mono text-[10px] uppercase text-zinc-500 mb-1">
                Swap Fee (bps, ≤500)
              </label>
              <input
                type="text"
                value={launchFeeBps}
                onChange={(e) => setLaunchFeeBps(e.target.value)}
                className="w-full rounded-lg border border-white/[0.1] bg-[#090c12] px-2.5 py-1.5 font-mono text-xs text-zinc-100"
              />
            </div>
            <div>
              <label className="block font-mono text-[10px] uppercase text-zinc-500 mb-1">Fee Routing</label>
              <select
                value={launchFeeMode}
                onChange={(e) => setLaunchFeeMode(e.target.value as 'holders' | 'treasury')}
                className="w-full rounded-lg border border-white/[0.1] bg-[#090c12] px-2 py-1.5 font-mono text-xs text-zinc-100"
              >
                <option value="holders">100% Holder Dividends</option>
                <option value="treasury">Custom Treasury Sink</option>
              </select>
            </div>
          </div>

          {launchFeeMode === 'treasury' && (
            <div>
              <label className="block font-mono text-[10px] uppercase text-zinc-500 mb-1">
                Immutable Fee Sink EVM Address
              </label>
              <input
                type="text"
                value={launchFeeSink}
                onChange={(e) => setLaunchFeeSink(e.target.value)}
                placeholder={evmAddress ?? '0x...'}
                className="w-full rounded-lg border border-white/[0.1] bg-[#090c12] px-2.5 py-1.5 font-mono text-xs text-zinc-100"
              />
            </div>
          )}

          <div>
            <label className="block font-mono text-[10px] uppercase text-zinc-500 mb-1">Metadata URI</label>
            <input
              type="text"
              value={launchUri}
              onChange={(e) => setLaunchUri(e.target.value)}
              className="w-full rounded-lg border border-white/[0.1] bg-[#090c12] px-2.5 py-1.5 font-mono text-xs text-zinc-100"
            />
          </div>

          <button
            type="button"
            onClick={() => void handleLaunchCurve()}
            disabled={launchBusy}
            className="w-full inline-flex items-center justify-center gap-2 rounded-xl bg-amber-400 px-4 py-2.5 text-xs font-semibold text-zinc-950 transition hover:bg-amber-300 cursor-pointer disabled:opacity-50"
          >
            {launchBusy ? <Loader2 className="h-4 w-4 animate-spin" /> : <Rocket className="h-4 w-4" />}
            <span>
              {evmAddress
                ? `Deploy ${launchSym.toUpperCase() || 'CURVE'} on Base Mainnet`
                : 'Connect Wallet to Deploy Fair Curve'}
            </span>
          </button>
        </div>
      </div>

      {/* Row 2: Live Base Mainnet PayWord Channel Lab + Zero-Dep SDK Downloads */}
      <div
        id="paywords"
        className="grid grid-cols-1 gap-5 lg:grid-cols-12 rounded-2xl border border-cyan-500/25 bg-cyan-500/[0.025] p-5 sm:p-6"
      >
        {/* Left 6 cols: Browser WebCrypto Chain Generator + 1-Click Open & Step */}
        <div className="lg:col-span-6 space-y-4">
          <div className="inline-flex items-center gap-2 font-mono text-[11px] uppercase tracking-[0.1em] text-cyan-300">
            <KeyRound className="h-4 w-4" />
            <span>Interactive Base USDC PayWord Channel Lab (WebCrypto)</span>
          </div>
          <p className="text-xs leading-relaxed text-zinc-400">
            Generate a real SHA-256 hash chain locally in your browser (seed never leaves your
            device), escrow USDC in{' '}
            <code className="font-mono text-zinc-200">{truncateHash(BASE_PAYWORD_CHANNEL_ADDRESS, 8, 6)}</code>,
            emit zero-gas <code className="font-mono text-zinc-200">O(1)</code> vouchers, and settle in
            1 Base tx.
          </p>

          <div className="grid grid-cols-2 gap-2.5">
            <div className="col-span-2">
              <label className="block font-mono text-[10px] uppercase text-zinc-500 mb-1">
                API Server Recipient (Base EVM Address)
              </label>
              <input
                type="text"
                value={pwRecipient}
                onChange={(e) => setPwRecipient(e.target.value)}
                className="w-full rounded-lg border border-white/[0.1] bg-[#090c12] px-2.5 py-1.5 font-mono text-xs text-zinc-100"
              />
            </div>
            <div>
              <label className="block font-mono text-[10px] uppercase text-zinc-500 mb-1">
                Price / Call (USDC)
              </label>
              <input
                type="text"
                value={pwUnitUsdc}
                onChange={(e) => setPwUnitUsdc(e.target.value)}
                className="w-full rounded-lg border border-white/[0.1] bg-[#090c12] px-2.5 py-1.5 font-mono text-xs text-zinc-100"
              />
            </div>
            <div>
              <label className="block font-mono text-[10px] uppercase text-zinc-500 mb-1">
                Chain Depth N (1..2048)
              </label>
              <input
                type="text"
                value={pwSteps}
                onChange={(e) => setPwSteps(e.target.value)}
                className="w-full rounded-lg border border-white/[0.1] bg-[#090c12] px-2.5 py-1.5 font-mono text-xs text-zinc-100"
              />
            </div>
            <div>
              <label className="block font-mono text-[10px] uppercase text-zinc-500 mb-1">
                Expiry (Hours)
              </label>
              <input
                type="text"
                value={pwExpiryHours}
                onChange={(e) => setPwExpiryHours(e.target.value)}
                className="w-full rounded-lg border border-white/[0.1] bg-[#090c12] px-2.5 py-1.5 font-mono text-xs text-zinc-100"
              />
            </div>
            <div className="flex items-end">
              <button
                type="button"
                onClick={() => void handleGenerateLocalChain()}
                disabled={pwBusy !== null}
                className="w-full inline-flex items-center justify-center gap-1.5 rounded-lg bg-cyan-400 px-3 py-1.5 text-xs font-semibold text-zinc-950 hover:bg-cyan-300 cursor-pointer disabled:opacity-50"
              >
                {pwBusy === 'gen' ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <KeyRound className="h-3.5 w-3.5" />}
                <span>1. Generate Chain</span>
              </button>
            </div>
          </div>

          {pwLocalChain && (
            <div className="rounded-xl border border-cyan-500/25 bg-[#090c12] p-3.5 font-mono text-[11px] space-y-2 text-zinc-300">
              <div className="flex items-center justify-between gap-2">
                <span className="text-zinc-500">Root w_0 (public):</span>
                <span className="inline-flex items-center gap-1 text-cyan-300">
                  {truncateHash(pwLocalChain.rootHex, 10, 8)}
                  <CopyButton value={pwLocalChain.rootHex} />
                </span>
              </div>
              {pwLocalChain.predictedChannelId && (
                <div className="flex items-center justify-between gap-2">
                  <span className="text-zinc-500">Deterministic Channel ID:</span>
                  <span className="inline-flex items-center gap-1 text-amber-300">
                    {truncateHash(pwLocalChain.predictedChannelId, 10, 8)}
                    <CopyButton value={pwLocalChain.predictedChannelId} />
                  </span>
                </div>
              )}
              <div className="flex flex-wrap gap-2 pt-1">
                <button
                  type="button"
                  onClick={() => void handleOpenPayWordChannel()}
                  disabled={pwBusy !== null}
                  className="inline-flex items-center gap-1.5 rounded-lg bg-emerald-400 px-3 py-1.5 text-xs font-semibold text-zinc-950 hover:bg-emerald-300 cursor-pointer disabled:opacity-50"
                >
                  {pwBusy === 'open' ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Zap className="h-3.5 w-3.5" />}
                  <span>2. Approve &amp; Open Channel On-Chain</span>
                </button>
                <button
                  type="button"
                  onClick={handleStepVoucher}
                  className="inline-flex items-center gap-1.5 rounded-lg border border-cyan-400/40 bg-cyan-400/10 px-3 py-1.5 text-xs font-semibold text-cyan-200 hover:bg-cyan-400/20 cursor-pointer"
                >
                  <span>3. Emit Step Voucher ({pwLocalChain.spentSteps}/{pwLocalChain.maxSteps})</span>
                </button>
              </div>
              {latestLocalPreimage && (
                <div className="rounded-lg border border-white/[0.07] bg-[#05070b] p-2 text-[10px] break-all text-emerald-300">
                  X-PayWord-Index: {pwLocalChain.spentSteps} · X-PayWord-Preimage: {latestLocalPreimage}
                </div>
              )}
            </div>
          )}
        </div>

        {/* Right 6 cols: Live On-Chain Channel Inspector + Settle + Zero-Dep SDK Downloads */}
        <div className="lg:col-span-6 space-y-4 lg:border-l lg:border-white/[0.08] lg:pl-5">
          <div className="inline-flex items-center gap-2 font-mono text-[11px] uppercase tracking-[0.1em] text-zinc-300">
            <Search className="h-4 w-4 text-cyan-300" />
            <span>Inspect &amp; Settle Any Live Base PayWord Channel</span>
          </div>

          <div className="flex gap-2">
            <input
              type="text"
              value={inspectId}
              onChange={(e) => setInspectId(e.target.value)}
              placeholder="0x... 32-byte Base PayWord channelId"
              className="flex-1 rounded-lg border border-white/[0.1] bg-[#090c12] px-2.5 py-1.5 font-mono text-xs text-zinc-100"
            />
            <button
              type="button"
              onClick={() => void handleInspectChannel()}
              disabled={inspectBusy}
              className="inline-flex items-center gap-1.5 rounded-lg border border-cyan-400/40 bg-cyan-400/15 px-3 py-1.5 text-xs font-semibold text-cyan-200 hover:bg-cyan-400/25 cursor-pointer disabled:opacity-50"
            >
              {inspectBusy ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Search className="h-3.5 w-3.5" />}
              <span>Inspect</span>
            </button>
          </div>

          {inspectRes && (
            <div className="rounded-xl border border-white/[0.08] bg-[#090c12] p-3.5 font-mono text-[11px] space-y-1.5 text-zinc-300">
              {!inspectRes.ok ? (
                <div className="text-rose-300">Error: {inspectRes.error}</div>
              ) : !inspectRes.found ? (
                <div className="text-amber-300">Not found on Base Mainnet: {inspectRes.reason}</div>
              ) : (
                <>
                  <div className="flex justify-between">
                    <span className="text-zinc-500">Status / Asset</span>
                    <span className={inspectRes.closed ? 'text-rose-300' : 'text-emerald-300'}>
                      {inspectRes.closed ? 'CLOSED' : 'ACTIVE'} · {inspectRes.token_symbol}
                    </span>
                  </div>
                  <div className="flex justify-between">
                    <span className="text-zinc-500">Payer → Recipient</span>
                    <span>
                      {truncateHash(inspectRes.payer ?? '', 6, 4)} → {truncateHash(inspectRes.recipient ?? '', 6, 4)}
                    </span>
                  </div>
                  <div className="flex justify-between">
                    <span className="text-zinc-500">Steps Claimed / Max</span>
                    <span>
                      {inspectRes.claimed_steps} / {inspectRes.max_steps} ({inspectRes.remaining_steps} left)
                    </span>
                  </div>
                  <div className="flex justify-between">
                    <span className="text-zinc-500">Remaining Escrowed Balance</span>
                    <span className="text-cyan-300">
                      ${(inspectRes.remaining_amount_usdc ?? 0).toFixed(6)} {inspectRes.token_symbol}
                    </span>
                  </div>
                </>
              )}
            </div>
          )}

          {/* 1-Click claim() / closeAndRefund() */}
          <div className="rounded-xl border border-white/[0.07] bg-white/[0.02] p-3 space-y-2">
            <div className="grid grid-cols-3 gap-2">
              <input
                type="text"
                value={claimStepsInput}
                onChange={(e) => setClaimStepsInput(e.target.value)}
                placeholder="Steps"
                className="rounded-md border border-white/[0.1] bg-[#05070b] px-2 py-1.5 font-mono text-xs text-zinc-100"
              />
              <input
                type="text"
                value={claimPreimageInput}
                onChange={(e) => setClaimPreimageInput(e.target.value)}
                placeholder="0x... preimage w_k"
                className="col-span-2 rounded-md border border-white/[0.1] bg-[#05070b] px-2 py-1.5 font-mono text-xs text-zinc-100"
              />
            </div>
            <div className="flex gap-2">
              <button
                type="button"
                onClick={() => void handleClaimChannel()}
                disabled={pwBusy !== null}
                className="flex-1 inline-flex items-center justify-center gap-1.5 rounded-lg bg-emerald-400 px-3 py-1.5 text-xs font-semibold text-zinc-950 hover:bg-emerald-300 cursor-pointer disabled:opacity-50"
              >
                {pwBusy === 'claim' ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Zap className="h-3.5 w-3.5" />}
                <span>Settle claim()</span>
              </button>
              <button
                type="button"
                onClick={() => void handleCloseChannel()}
                disabled={pwBusy !== null}
                className="inline-flex items-center justify-center gap-1.5 rounded-lg border border-white/[0.12] bg-white/[0.05] px-3 py-1.5 text-xs font-medium text-zinc-200 hover:bg-white/[0.1] cursor-pointer disabled:opacity-50"
              >
                <span>closeAndRefund()</span>
              </button>
            </div>
          </div>

          {/* Direct Zero-Dependency SDK Downloads */}
          <div className="rounded-xl border border-emerald-500/20 bg-emerald-500/[0.04] p-3.5 space-y-2">
            <div className="flex items-center justify-between">
              <span className="font-mono text-[11px] font-semibold text-emerald-300">
                Zero-Dependency Production SDKs (v0.30.0)
              </span>
              <span className="font-mono text-[10px] text-zinc-400">L1 LOVE + Base USDC</span>
            </div>
            <p className="text-[11px] leading-relaxed text-zinc-400">
              Single-file ESM JS (Node 18+ / Cloudflare Workers / Deno / Bun) &amp; Python 3.9+ stdlib
              SDKs with `createBasePayWordGate`, `BasePayWordGate`, and exact ABI calldata encoders:
            </p>
            <div className="flex flex-wrap gap-2 pt-0.5">
              <a
                href="/sdk/newsatoshi-402.mjs"
                download="newsatoshi-402.mjs"
                className="inline-flex items-center gap-1.5 rounded-lg border border-emerald-500/30 bg-[#090c12] px-3 py-1.5 font-mono text-[11px] text-emerald-200 hover:bg-emerald-500/15"
              >
                <Download className="h-3.5 w-3.5" />
                <span>newsatoshi-402.mjs</span>
              </a>
              <a
                href="/sdk/newsatoshi-402.py"
                download="newsatoshi_402.py"
                className="inline-flex items-center gap-1.5 rounded-lg border border-cyan-500/30 bg-[#090c12] px-3 py-1.5 font-mono text-[11px] text-cyan-200 hover:bg-cyan-500/15"
              >
                <Download className="h-3.5 w-3.5" />
                <span>newsatoshi_402.py</span>
              </a>
            </div>
          </div>
        </div>
      </div>
    </section>
  );
}
