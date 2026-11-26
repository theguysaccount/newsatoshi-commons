import { useEffect, useMemo, useState } from 'react';
import {
  AlertTriangle,
  ArrowLeftRight,
  ArrowUpRight,
  CheckCircle2,
  ChevronDown,
  ChevronUp,
  Cpu,
  Download,
  KeyRound,
  Loader2,
  Pickaxe,
  Plus,
  Radio,
  SearchCheck,
  ShieldCheck,
  Sparkles,
  Terminal,
  Wallet,
  Zap,
} from 'lucide-react';
import { actionCompileSwap, actionPublishSwap, actionVerifySwap } from '../api/client';
import type {
  AtomicSwapContract,
  BackingProofSummary,
  CommonsState,
  CompileSwapPayload,
  DualChainSwapVerification,
  SeedStatusResponse,
} from '../types';
import {
  BASE_WLOVE_ADDRESS,
  BASE_WLOVE_POOL,
  CANONICAL_HTLC,
  EVM_CHAINS,
  encodeApprove,
  encodeHtlcLockErc20,
  encodeHtlcLockEth,
  parseUnits,
  readErc20Allowance,
  type EvmChainKey,
} from '../wallet/evm';
import { useWallet } from '../wallet/WalletContext';
import { InstantSwapCard } from './InstantSwapCard';
import { QuickBackCard } from './QuickBackCard';
import { CopyButton, HashPill, truncateHash } from './ui/CopyButton';
import { Panel } from './ui/Panel';

export type SwapHubSection = 'swap' | 'back' | 'bridge' | 'setup';

interface SwapsMiningTabProps {
  state: CommonsState | null;
  seed: SeedStatusResponse | null;
  backing: BackingProofSummary | null;
  hasOperatorToken: boolean;
  initialSection?: SwapHubSection;
  onRefreshBacking: (force?: boolean) => void;
  onStateUpdated: (next: CommonsState) => void;
  onNotice: (kind: 'success' | 'error', message: string) => void;
}

function bytesToHex(bytes: Uint8Array): string {
  return Array.from(bytes)
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('');
}

const MCP_SNIPPET = `{
  "mcpServers": {
    "newsatoshi": {
      "url": "https://commons.newsatoshi.org/mcp"
    }
  }
}`;

export function SwapsMiningTab({
  state,
  seed,
  backing,
  hasOperatorToken,
  initialSection = 'swap',
  onRefreshBacking,
  onStateUpdated,
  onNotice,
}: SwapsMiningTabProps) {
  const {
    evmAddress,
    evmChainId,
    openModal,
    switchEvmChain,
    sendEvmTx,
    waitForEvmReceipt,
    addWloveToWallet,
    solanaAddress,
    connectSolana,
    btcAddress,
    btcPubkeyHex,
    connectBtc,
    loveAddress,
    setLoveAddress,
  } = useWallet();

  const [section, setSection] = useState<SwapHubSection>(initialSection);

  useEffect(() => {
    setSection(initialSection);
  }, [initialSection]);

  // ── Guided 1-Click EVM HTLC Bridge State ───────────────────────────────────
  const [guidedChain, setGuidedChain] = useState<EvmChainKey>('base');
  const [guidedAsset, setGuidedAsset] = useState<'NATIVE' | 'USDC' | 'WLOVE'>('NATIVE');
  const [guidedAmount, setGuidedAmount] = useState<string>('0.002');
  const [guidedCounterparty, setGuidedCounterparty] = useState<string>('');
  const [guidedTimeoutHours, setGuidedTimeoutHours] = useState<number>(6);
  const [localPreimage, setLocalPreimage] = useState<string>('');
  const [paymentHash, setPaymentHash] = useState<string>('');
  const [lockingHtlc, setLockingHtlc] = useState<string | null>(null);
  const [htlcLockTx, setHtlcLockTx] = useState<string | null>(null);

  // ── Collapsible Advanced Power-User HTLC Compiler & Verifier ───────────────
  const [showAdvanced, setShowAdvanced] = useState<boolean>(false);
  const [rail, setRail] = useState<'btc_onchain' | 'lightning_bolt11' | 'solana_htlc' | 'evm_htlc'>(
    'evm_htlc'
  );
  const [direction, setDirection] = useState<'btc_to_love' | 'love_to_btc'>('btc_to_love');
  const [loveNet, setLoveNet] = useState<'mainnet' | 'testnet'>('mainnet');
  const [btcNet, setBtcNet] = useState<'mainnet' | 'testnet' | 'signet' | 'regtest'>('mainnet');
  const [bolt11Invoice, setBolt11Invoice] = useState<string>('');
  const [amountLove, setAmountLove] = useState<string>('');
  const [amountBtcSats, setAmountBtcSats] = useState<string>('');
  const [loveClaimAddr, setLoveClaimAddr] = useState<string>(loveAddress);
  const [loveTimeoutBlocks, setLoveTimeoutBlocks] = useState<string>('144');
  const [btcClaimPk, setBtcClaimPk] = useState<string>('');
  const [btcRefundPk, setBtcRefundPk] = useState<string>('');
  const [btcLocktime, setBtcLocktime] = useState<string>('');
  const [btcCurrentHeight, setBtcCurrentHeight] = useState<string>('');
  const [sourceAsset, setSourceAsset] = useState<string>('USDC');
  const [solCluster, setSolCluster] = useState<string>('mainnet-beta');
  const [solSender, setSolSender] = useState<string>('');
  const [solRecipient, setSolRecipient] = useState<string>('');
  const [evmChain, setEvmChain] = useState<string>('base');
  const [evmRecipient, setEvmRecipient] = useState<string>('');
  const [makerLabel, setMakerLabel] = useState<string>('');
  const [compiling, setCompiling] = useState(false);
  const [publishing, setPublishing] = useState(false);
  const [compiled, setCompiled] = useState<AtomicSwapContract | null>(null);
  const [compileError, setCompileError] = useState<string | null>(null);

  // Live Dual-Chain Verifier state
  const [verifyHash, setVerifyHash] = useState<string>('');
  const [verifyLoveNet, setVerifyLoveNet] = useState<'mainnet' | 'testnet'>('mainnet');
  const [verifyBtcAddr, setVerifyBtcAddr] = useState<string>('');
  const [verifyPreimage, setVerifyPreimage] = useState<string>('');
  const [verifying, setVerifying] = useState(false);
  const [verifyResult, setVerifyResult] = useState<DualChainSwapVerification | null>(null);
  const [verifyError, setVerifyError] = useState<string | null>(null);

  // Easy Setup & Mining state
  const [minerThs, setMinerThs] = useState<string>('1.2');
  const [poolStatus, setPoolStatus] = useState<{
    ok?: boolean;
    online?: boolean;
    stratum_url?: string;
    pool_fee_pct?: number;
    connected_workers?: number;
    estimated_pool_hashrate_ghs?: number;
    shares_accepted?: number;
    aux_blocks_accepted?: number;
    current_template?: {
      aux_height?: number;
      worker_difficulty?: number;
      network_difficulty?: number;
    } | null;
  } | null>(null);
  const [mcpTestBusy, setMcpTestBusy] = useState(false);

  useEffect(() => {
    if (section !== 'setup') return;
    let alive = true;
    const pollPool = async () => {
      try {
        const res = await fetch('/api/pool/status');
        if (res.ok && alive) {
          const data = await res.json();
          setPoolStatus(data);
        }
      } catch {
        // silent
      }
    };
    void pollPool();
    const id = window.setInterval(() => void pollPool(), 15000);
    return () => {
      alive = false;
      window.clearInterval(id);
    };
  }, [section]);
  const [mcpTestResult, setMcpTestResult] = useState<{
    ok: boolean;
    toolCount?: number;
    latencyMs?: number;
    error?: string;
  } | null>(null);
  const [loveDraft, setLoveDraft] = useState(loveAddress);

  // Auto-sync connected wallet addresses into advanced bridge fields so users never copy/paste
  useEffect(() => {
    if (loveAddress && !loveClaimAddr) setLoveClaimAddr(loveAddress);
    if (loveAddress && !loveDraft) setLoveDraft(loveAddress);
  }, [loveAddress, loveClaimAddr, loveDraft]);

  useEffect(() => {
    if (solanaAddress && !solSender) setSolSender(solanaAddress);
  }, [solanaAddress, solSender]);

  useEffect(() => {
    if (btcPubkeyHex && !btcRefundPk) setBtcRefundPk(btcPubkeyHex);
  }, [btcPubkeyHex, btcRefundPk]);

  const handleGenerateLocalSecret = async (): Promise<{ preimage: string; hash: string } | null> => {
    try {
      const preimage = new Uint8Array(32);
      window.crypto.getRandomValues(preimage);
      const digest = new Uint8Array(await window.crypto.subtle.digest('SHA-256', preimage));
      const pHex = bytesToHex(preimage);
      const hHex = bytesToHex(digest);
      setLocalPreimage(pHex);
      setPaymentHash(hHex);
      setVerifyHash(hHex);
      onNotice('success', 'Generated 32-byte SHA-256 secret locally (never leaves your browser)');
      return { preimage: pHex, hash: hHex };
    } catch {
      onNotice('error', 'Web Crypto CSPRNG unavailable in this browser context');
      return null;
    }
  };

  const handleDownloadSecretReceipt = () => {
    if (!localPreimage || !paymentHash) return;
    const payload = {
      protocol: 'NEWSATOSHI/2.1 SHA-256 HTLC Secret Receipt',
      created_at: new Date().toISOString(),
      keep_private_preimage_hex: localPreimage,
      public_payment_hash_hex: paymentHash,
      evm_htlc_contract: CANONICAL_HTLC,
      chain: guidedChain,
      asset: guidedAsset,
      amount: guidedAmount,
      counterparty: guidedCounterparty || null,
      love_claim_address: loveAddress || null,
      tx_hash: htlcLockTx || null,
    };
    const blob = new Blob([JSON.stringify(payload, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `newsatoshi-htlc-secret-${paymentHash.slice(0, 8)}.json`;
    a.click();
    URL.revokeObjectURL(url);
  };

  const guidedCfg = EVM_CHAINS[guidedChain];
  const guidedDecimals =
    guidedAsset === 'USDC' ? 6 : guidedAsset === 'WLOVE' ? 18 : guidedCfg.nativeDecimals;
  const guidedSymbol =
    guidedAsset === 'NATIVE'
      ? guidedCfg.nativeSymbol
      : guidedAsset === 'USDC'
        ? 'USDC'
        : 'wLOVE';
  const guidedAmountRaw = useMemo(
    () => parseUnits(guidedAmount, guidedDecimals),
    [guidedAmount, guidedDecimals]
  );

  const handleLockEvmHtlcDirect = async () => {
    if (!evmAddress) {
      openModal();
      return;
    }
    if (evmChainId !== guidedCfg.chainId) {
      setLockingHtlc(`Switching wallet to ${guidedCfg.name}…`);
      try {
        await switchEvmChain(guidedChain);
      } finally {
        setLockingHtlc(null);
      }
      return;
    }
    if (!/^0x[0-9a-fA-F]{40}$/.test(guidedCounterparty.trim())) {
      onNotice('error', 'Enter a valid 0x… counterparty maker address');
      return;
    }
    if (guidedCounterparty.trim().toLowerCase() === evmAddress.toLowerCase()) {
      onNotice('error', 'Counterparty must be a different address from your own wallet');
      return;
    }
    if (!guidedAmountRaw || guidedAmountRaw <= 0n) {
      onNotice('error', 'Enter an amount greater than 0');
      return;
    }

    let hashHex = paymentHash.trim();
    if (!/^[0-9a-fA-F]{64}$/.test(hashHex)) {
      const gen = await handleGenerateLocalSecret();
      if (!gen) return;
      hashHex = gen.hash;
    }

    const timelockUnix = Math.floor(Date.now() / 1000) + guidedTimeoutHours * 3600;
    setHtlcLockTx(null);

    try {
      if (guidedAsset === 'NATIVE') {
        setLockingHtlc(`Locking ${guidedAmount} ${guidedSymbol} in HTLC…`);
        const txHash = await sendEvmTx({
          to: CANONICAL_HTLC,
          data: encodeHtlcLockEth(guidedCounterparty.trim(), `0x${hashHex}`, timelockUnix),
          value: `0x${guidedAmountRaw.toString(16)}`,
        });
        setHtlcLockTx(txHash);
        onNotice('success', `HTLC lock submitted on ${guidedCfg.name}! Save your secret receipt.`);
        await waitForEvmReceipt(guidedChain, txHash);
      } else {
        const tokenAddr =
          guidedAsset === 'WLOVE' ? BASE_WLOVE_ADDRESS : guidedCfg.usdcAddress;
        setLockingHtlc(`Checking ${guidedSymbol} allowance…`);
        const allowance = await readErc20Allowance(
          guidedChain,
          tokenAddr,
          evmAddress,
          CANONICAL_HTLC
        );
        if (allowance < guidedAmountRaw) {
          setLockingHtlc(`Approving ${guidedSymbol} for HTLC…`);
          const appTx = await sendEvmTx({
            to: tokenAddr,
            data: encodeApprove(CANONICAL_HTLC, guidedAmountRaw),
            value: '0x0',
          });
          await waitForEvmReceipt(guidedChain, appTx);
        }
        setLockingHtlc(`Locking ${guidedAmount} ${guidedSymbol} in HTLC…`);
        const txHash = await sendEvmTx({
          to: CANONICAL_HTLC,
          data: encodeHtlcLockErc20(
            guidedCounterparty.trim(),
            tokenAddr,
            guidedAmountRaw,
            `0x${hashHex}`,
            timelockUnix
          ),
          value: '0x0',
        });
        setHtlcLockTx(txHash);
        onNotice('success', `${guidedSymbol} HTLC lock submitted on ${guidedCfg.name}!`);
        await waitForEvmReceipt(guidedChain, txHash);
      }
    } catch (err) {
      onNotice('error', err instanceof Error ? err.message : 'HTLC lock cancelled');
    } finally {
      setLockingHtlc(null);
    }
  };

  // Advanced compiler handlers
  const buildPayload = (): CompileSwapPayload => {
    const satsNum = amountBtcSats.trim() ? Number(amountBtcSats.trim()) : undefined;
    const lockNum = btcLocktime.trim() ? Number(btcLocktime.trim()) : undefined;
    const curHeightNum = btcCurrentHeight.trim() ? Number(btcCurrentHeight.trim()) : undefined;
    return {
      rail,
      direction,
      love_network: loveNet,
      btc_network: btcNet,
      payment_hash_hex: rail !== 'lightning_bolt11' ? paymentHash.trim() : undefined,
      bolt11_invoice: rail === 'lightning_bolt11' ? bolt11Invoice.trim() : undefined,
      amount_love: amountLove.trim(),
      amount_btc_sats: satsNum && Number.isFinite(satsNum) ? Math.floor(satsNum) : undefined,
      love_claim_address: loveClaimAddr.trim(),
      love_timeout_blocks: Math.max(1, Number(loveTimeoutBlocks) || 144),
      btc_claim_pubkey_hex: rail === 'btc_onchain' ? btcClaimPk.trim() : undefined,
      btc_refund_pubkey_hex: rail === 'btc_onchain' ? btcRefundPk.trim() : undefined,
      btc_locktime: rail === 'btc_onchain' && lockNum ? Math.floor(lockNum) : undefined,
      btc_current_height: rail === 'btc_onchain' && curHeightNum ? Math.floor(curHeightNum) : undefined,
      source_asset: rail === 'solana_htlc' || rail === 'evm_htlc' ? sourceAsset : undefined,
      solana_cluster: rail === 'solana_htlc' ? solCluster : undefined,
      solana_sender_pubkey: rail === 'solana_htlc' ? solSender.trim() : undefined,
      solana_recipient_pubkey: rail === 'solana_htlc' ? solRecipient.trim() : undefined,
      evm_chain: rail === 'evm_htlc' ? evmChain : undefined,
      evm_recipient_address: rail === 'evm_htlc' ? evmRecipient.trim() : undefined,
      maker_label: makerLabel.trim() || undefined,
    };
  };

  const handleCompile = async (e: React.FormEvent) => {
    e.preventDefault();
    setCompiling(true);
    setCompileError(null);
    try {
      const res = await actionCompileSwap(buildPayload());
      if (res.ok && res.contract) {
        setCompiled(res.contract);
        setVerifyHash(res.contract.payment_hash_hex);
        setVerifyLoveNet(res.contract.love_network === 'testnet' ? 'testnet' : 'mainnet');
        if (res.contract.btc_htlc?.btc_p2wsh_address) {
          setVerifyBtcAddr(res.contract.btc_htlc.btc_p2wsh_address);
        }
        onNotice('success', `Compiled canonical ${res.contract.rail} HTLC swap (${res.contract.swap_id})`);
      } else {
        setCompiled(null);
        setCompileError(res.error || 'Swap compilation failed');
      }
    } catch (err) {
      setCompiled(null);
      setCompileError(err instanceof Error ? err.message : 'Swap compilation error');
    } finally {
      setCompiling(false);
    }
  };

  const handlePublish = async () => {
    setPublishing(true);
    try {
      const res = await actionPublishSwap(buildPayload());
      if (res.state) onStateUpdated(res.state);
      if (res.ok && res.contract) {
        setCompiled(res.contract);
        onNotice('success', `Published non-custodial swap ${res.contract.swap_id} to live board`);
      } else {
        onNotice('error', res.error || 'Publish failed');
      }
    } catch (err) {
      onNotice('error', err instanceof Error ? err.message : 'Publish swap error');
    } finally {
      setPublishing(false);
    }
  };

  const handleVerifyBothChains = async (e: React.FormEvent) => {
    e.preventDefault();
    setVerifying(true);
    setVerifyError(null);
    try {
      const res = await actionVerifySwap({
        payment_hash_hex: verifyHash.trim(),
        love_network: verifyLoveNet,
        btc_address: verifyBtcAddr.trim() || undefined,
        preimage_hex: verifyPreimage.trim() || undefined,
      });
      if (res.state) onStateUpdated(res.state);
      if (res.ok && res.verification) {
        setVerifyResult(res.verification);
        onNotice(
          'success',
          res.verification.love_confirmed
            ? `Confirmed on Newsatoshi ${res.verification.love_network} L1`
            : `Checked Newsatoshi ${res.verification.love_network} L1`
        );
      } else {
        setVerifyResult(null);
        setVerifyError(res.error || 'Verification failed');
      }
    } catch (err) {
      setVerifyResult(null);
      setVerifyError(err instanceof Error ? err.message : 'Dual-chain verification failed');
    } finally {
      setVerifying(false);
    }
  };

  const handleTestLiveMcp = async () => {
    setMcpTestBusy(true);
    setMcpTestResult(null);
    const t0 = performance.now();
    try {
      const res = await fetch('/mcp', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'tools/list', params: {} }),
      });
      const data = (await res.json()) as { result?: { tools?: unknown[] }; error?: { message?: string } };
      const latencyMs = Math.round(performance.now() - t0);
      if (res.ok && Array.isArray(data.result?.tools)) {
        setMcpTestResult({ ok: true, toolCount: data.result.tools.length, latencyMs });
        onNotice('success', `MCP live: ${data.result.tools.length} tools responded in ${latencyMs}ms`);
      } else {
        setMcpTestResult({ ok: false, error: data.error?.message || `HTTP ${res.status}` });
      }
    } catch (err) {
      setMcpTestResult({ ok: false, error: err instanceof Error ? err.message : 'MCP request failed' });
    } finally {
      setMcpTestBusy(false);
    }
  };

  const mainnet = seed?.mainnet_status;
  const swapOffers = state?.swap_offers ?? [];

  const dockerQuickstart = `git clone https://github.com/theguysaccount/newsatoshi-commons.git
cd newsatoshi-commons/auxpow-stratum
export NS_RPC_AUTH="auxpow:$(openssl rand -hex 16)"
docker compose up -d`;

  const cliInstallCmd = `curl -fsSL https://commons.newsatoshi.org/install.sh | sh`;

  const meterDaemonCmd = `agent-vault env-run \\
  --map COMMONS_HARDWARE_XMSS_SEED=newsatoshi/hardware-tee-xmss-seed \\
  --map COMMONS_HARDWARE_SLH_SEED=newsatoshi/hardware-tee-slh-seed -- \\
  newsatoshi-hw-signer poll-meter \\
    --keyfile ./hw-signer-state.json --env-seeds \\
    --adapter shelly-em --meter-url http://192.168.1.50/rpc/EMData.GetStatus?id=0 \\
    --soil 92 --love-per-kwh 10 --scheme slh_dsa \\
    --endpoint https://commons.newsatoshi.org/api/telemetry`;

  const TABS: { id: SwapHubSection; label: string }[] = [
    { id: 'swap', label: 'Instant Swap' },
    { id: 'back', label: '1-Click Back' },
    { id: 'bridge', label: 'Bridge to L1' },
    { id: 'setup', label: 'Easy Setup & Mine' },
  ];

  return (
    <div className="space-y-6">
      {/* Crisp 4-Mode Switcher + Connected Wallet Status */}
      <div className="flex flex-wrap items-center justify-between gap-3 rounded-2xl border border-white/[0.08] bg-[#0f0f12] px-4 py-3">
        <div className="flex flex-wrap items-center gap-1 rounded-full border border-white/[0.08] bg-[#0a0a0d] p-1">
          {TABS.map((t) => (
            <button
              key={t.id}
              type="button"
              onClick={() => setSection(t.id)}
              className={`rounded-full px-3.5 py-1.5 text-xs font-medium transition cursor-pointer ${
                section === t.id
                  ? 'bg-zinc-100 text-zinc-950'
                  : 'text-zinc-400 hover:text-zinc-100'
              }`}
            >
              {t.label}
            </button>
          ))}
        </div>

        <button
          type="button"
          onClick={openModal}
          className="inline-flex items-center gap-2 rounded-full border border-white/[0.12] bg-white/[0.03] px-3.5 py-1.5 text-xs font-medium text-zinc-200 transition hover:border-white/[0.25] hover:bg-white/[0.07] cursor-pointer"
        >
          <span
            className={`h-1.5 w-1.5 rounded-full ${evmAddress ? 'bg-emerald-400' : 'bg-zinc-500'}`}
          />
          <Wallet className="h-3.5 w-3.5 text-zinc-400" />
          <span className="font-mono text-[11px]">
            {evmAddress ? truncateHash(evmAddress, 6, 4) : 'Connect Wallet'}
          </span>
        </button>
      </div>

      {/* ═══════════════════════════════════════════════════════════════════════ */}
      {/* TAB 1 (DEFAULT): INSTANT 1-CLICK SWAP (ZERO FORM FILLING)              */}
      {/* ═══════════════════════════════════════════════════════════════════════ */}
      {section === 'swap' && (
        <div className="grid grid-cols-1 items-start gap-6 lg:grid-cols-12">
          <div className="lg:col-span-6 xl:col-span-5">
            <InstantSwapCard
              onNotice={onNotice}
              onOpenBridge={() => setSection('bridge')}
            />
          </div>

          <div className="space-y-4 lg:col-span-6 xl:col-span-7">
            {/* 30-Second Guided Checklist */}
            <div className="surface-card rounded-2xl p-6">
              <div className="flex items-center gap-2 text-xs font-medium text-zinc-400">
                <Sparkles className="h-3.5 w-3.5 text-emerald-400" />
                <span>3 Steps · Zero Forms</span>
              </div>
              <h2 className="font-display mt-2 text-3xl tracking-tight text-zinc-100">
                Swap in from any Base wallet in seconds.
              </h2>
              <p className="mt-2 text-xs leading-relaxed text-zinc-400">
                Connect MetaMask, Coinbase Wallet, Rabby, or Phantom — pick ETH or USDC, and swap directly through the ownerless, LP-burned Uniswap V3 pool on Base Mainnet.
              </p>

              <div className="mt-5 space-y-2.5">
                <div className="flex items-center justify-between gap-3 rounded-xl border border-white/[0.07] bg-[#0a0a0d] px-4 py-3">
                  <div className="flex items-center gap-3">
                    <span
                      className={`flex h-6 w-6 shrink-0 items-center justify-center rounded-full font-mono text-[11px] font-semibold ${
                        evmAddress
                          ? 'bg-emerald-500/20 text-emerald-300'
                          : 'bg-white/[0.08] text-zinc-300'
                      }`}
                    >
                      {evmAddress ? '✓' : '1'}
                    </span>
                    <div>
                      <div className="text-xs font-medium text-zinc-100">Connect your wallet</div>
                      <div className="text-[11px] text-zinc-500">
                        {evmAddress
                          ? `Connected: ${truncateHash(evmAddress, 6, 4)}`
                          : 'Auto-detects MetaMask, Coinbase Wallet, Rabby & Phantom'}
                      </div>
                    </div>
                  </div>
                  <button
                    type="button"
                    onClick={openModal}
                    className="shrink-0 rounded-lg border border-white/[0.12] px-3 py-1.5 text-[11px] font-medium text-zinc-200 hover:bg-white/[0.06] cursor-pointer"
                  >
                    {evmAddress ? 'Switch' : 'Connect'}
                  </button>
                </div>

                <div className="flex items-center justify-between gap-3 rounded-xl border border-white/[0.07] bg-[#0a0a0d] px-4 py-3">
                  <div className="flex items-center gap-3">
                    <span className="flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-white/[0.08] font-mono text-[11px] font-semibold text-zinc-300">
                      2
                    </span>
                    <div>
                      <div className="text-xs font-medium text-zinc-100">
                        Swap ETH or USDC → wLOVE
                      </div>
                      <div className="text-[11px] text-zinc-500">
                        0% buy/sell tax · 0 team allocation · LP NFT #6689936 burned forever
                      </div>
                    </div>
                  </div>
                  {evmAddress && (
                    <button
                      type="button"
                      onClick={() => void addWloveToWallet()}
                      className="inline-flex shrink-0 items-center gap-1 rounded-lg border border-emerald-500/30 bg-emerald-500/10 px-2.5 py-1.5 text-[11px] font-medium text-emerald-300 hover:bg-emerald-500/20 cursor-pointer"
                    >
                      <Plus className="h-3 w-3" />
                      <span>Add Token</span>
                    </button>
                  )}
                </div>

                <div className="flex items-center justify-between gap-3 rounded-xl border border-white/[0.07] bg-[#0a0a0d] px-4 py-3">
                  <div className="flex items-center gap-3">
                    <span
                      className={`flex h-6 w-6 shrink-0 items-center justify-center rounded-full font-mono text-[11px] font-semibold ${
                        loveAddress
                          ? 'bg-emerald-500/20 text-emerald-300'
                          : 'bg-white/[0.08] text-zinc-300'
                      }`}
                    >
                      {loveAddress ? '✓' : '3'}
                    </span>
                    <div>
                      <div className="text-xs font-medium text-zinc-100">
                        Want native shielded L1 LOVE?
                      </div>
                      <div className="text-[11px] text-zinc-500">
                        Open the zero-install browser wallet on newsatoshi.org, or use the 1-click Bridge tab.
                      </div>
                    </div>
                  </div>
                  <a
                    href="https://newsatoshi.org/wallet/"
                    target="_blank"
                    rel="noreferrer"
                    className="inline-flex shrink-0 items-center gap-1 rounded-lg border border-white/[0.12] px-2.5 py-1.5 text-[11px] font-medium text-zinc-200 hover:bg-white/[0.06]"
                  >
                    <span>L1 Wallet</span>
                    <ArrowUpRight className="h-3 w-3" />
                  </a>
                </div>
              </div>
            </div>

            {/* Verified Token & Pool Proof Card */}
            <div className="surface-card rounded-2xl p-5 text-xs">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <span className="font-mono text-[11px] uppercase tracking-wider text-zinc-400">
                  Verified Base wLOVE Contract
                </span>
                <span className="rounded-full border border-emerald-500/30 bg-emerald-500/10 px-2 py-0.5 font-mono text-[10px] text-emerald-300">
                  Sourcify Exact Match · 0 Owner
                </span>
              </div>
              <div className="mt-2.5 flex flex-wrap items-center justify-between gap-2 rounded-xl border border-white/[0.07] bg-[#0a0a0d] px-3.5 py-2.5 font-mono text-[11px] text-zinc-200">
                <span className="break-all">{BASE_WLOVE_ADDRESS}</span>
                <CopyButton value={BASE_WLOVE_ADDRESS} label="Copy" />
              </div>
              <div className="mt-3 flex flex-wrap items-center gap-2">
                <a
                  href={`https://app.uniswap.org/swap?chain=base&outputCurrency=${BASE_WLOVE_ADDRESS}`}
                  target="_blank"
                  rel="noreferrer"
                  className="inline-flex items-center gap-1 rounded-lg border border-white/[0.08] bg-[#0a0a0d] px-3 py-1.5 text-[11px] font-medium text-zinc-300 hover:border-white/[0.2] hover:text-white"
                >
                  <span>Trade on Uniswap</span>
                  <ArrowUpRight className="h-3 w-3" />
                </a>
                <a
                  href={`https://dexscreener.com/base/${BASE_WLOVE_POOL}`}
                  target="_blank"
                  rel="noreferrer"
                  className="inline-flex items-center gap-1 rounded-lg border border-white/[0.08] bg-[#0a0a0d] px-3 py-1.5 text-[11px] font-medium text-zinc-300 hover:border-white/[0.2] hover:text-white"
                >
                  <span>DEXScreener</span>
                  <ArrowUpRight className="h-3 w-3" />
                </a>
                <a
                  href={`https://www.geckoterminal.com/base/pools/${BASE_WLOVE_POOL.toLowerCase()}`}
                  target="_blank"
                  rel="noreferrer"
                  className="inline-flex items-center gap-1 rounded-lg border border-white/[0.08] bg-[#0a0a0d] px-3 py-1.5 text-[11px] font-medium text-zinc-300 hover:border-white/[0.2] hover:text-white"
                >
                  <span>GeckoTerminal</span>
                  <ArrowUpRight className="h-3 w-3" />
                </a>
                <a
                  href={`https://basescan.org/tx/0x378a7f5bc92bc777aa991246fa3aa0d2bc713577cf1f9bc50dc7ea8c0331c4bc`}
                  target="_blank"
                  rel="noreferrer"
                  className="inline-flex items-center gap-1 rounded-lg border border-white/[0.08] bg-[#0a0a0d] px-3 py-1.5 text-[11px] font-medium text-zinc-300 hover:border-white/[0.2] hover:text-white"
                >
                  <span>LP Burn Proof</span>
                  <ArrowUpRight className="h-3 w-3" />
                </a>
              </div>
            </div>
          </div>
        </div>
      )}

      {/* ═══════════════════════════════════════════════════════════════════════ */}
      {/* TAB 2: 1-CLICK MULTI-CHAIN BACKING VAULT                               */}
      {/* ═══════════════════════════════════════════════════════════════════════ */}
      {section === 'back' && (
        <div className="grid grid-cols-1 items-start gap-6 lg:grid-cols-12">
          <div className="lg:col-span-6">
            <QuickBackCard
              onBacked={() => onRefreshBacking(true)}
              onNotice={onNotice}
            />
          </div>

          <div className="surface-card rounded-2xl p-5 lg:col-span-6">
            <div className="flex items-center justify-between gap-2">
              <h3 className="text-[14px] font-semibold text-zinc-100">
                Live 4-Chain Reserve Breakdown
              </h3>
              <a
                href="/api/backing/summary"
                target="_blank"
                rel="noreferrer"
                className="inline-flex items-center gap-1 text-[11px] text-zinc-400 hover:text-zinc-100"
              >
                <span>Live RPC JSON</span>
                <ArrowUpRight className="h-3 w-3" />
              </a>
            </div>
            <p className="mt-1 text-xs text-zinc-500">
              Verified directly from <code className="font-mono text-zinc-300">0x37Ed…9265</code> across Base, Arbitrum, Optimism &amp; Polygon.
            </p>

            <div className="mt-4 grid grid-cols-1 gap-2.5 sm:grid-cols-2">
              {(backing?.chains ?? []).map((c) => (
                <div
                  key={c.chain_id}
                  className="rounded-xl border border-white/[0.07] bg-[#0a0a0d] p-3.5 text-xs"
                >
                  <div className="flex items-center justify-between">
                    <span className="font-medium capitalize text-zinc-100">{c.chain}</span>
                    <span className="font-mono text-[10px] text-emerald-400">
                      {c.live ? 'Live' : 'Syncing'}
                    </span>
                  </div>
                  <div className="mt-2 font-mono text-sm font-medium text-zinc-100 tabular-nums">
                    {c.native_reserve_formatted} {c.native_symbol}
                  </div>
                  <div className="mt-0.5 font-mono text-[11px] text-zinc-400">
                    {c.usdc_reserve_formatted} USDC · {c.deposits_count} deposit{c.deposits_count === 1 ? '' : 's'}
                  </div>
                </div>
              ))}
            </div>
          </div>
        </div>
      )}

      {/* ═══════════════════════════════════════════════════════════════════════ */}
      {/* TAB 3: GUIDED 1-CLICK EVM HTLC BRIDGE + COLLAPSIBLE ADVANCED COMPILER  */}
      {/* ═══════════════════════════════════════════════════════════════════════ */}
      {section === 'bridge' && (
        <div className="space-y-6">
          <div className="grid grid-cols-1 items-start gap-6 lg:grid-cols-12">
            {/* Simple Guided EVM HTLC Locker */}
            <div className="surface-card rounded-2xl p-5 lg:col-span-7">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <div className="flex items-center gap-2">
                  <ArrowLeftRight className="h-4 w-4 text-emerald-400" />
                  <h3 className="text-[14px] font-semibold text-zinc-100">
                    Guided Atomic Bridge (Zero Custody)
                  </h3>
                </div>
                <span className="rounded-full border border-emerald-500/30 bg-emerald-500/10 px-2.5 py-0.5 font-mono text-[10px] text-emerald-300">
                  SHA-256 HTLC · 0xF564…987
                </span>
              </div>
              <p className="mt-1.5 text-xs text-zinc-400">
                Lock ETH, USDC, or wLOVE directly in the ownerless SHA-256 HTLC contract from your connected wallet — no manual hex, no base-unit math, automatic refund if not claimed.
              </p>

              {/* Step 1: Chain & Asset */}
              <div className="mt-4 space-y-3">
                <div className="grid grid-cols-4 gap-1.5">
                  {(Object.keys(EVM_CHAINS) as EvmChainKey[]).map((key) => {
                    const item = EVM_CHAINS[key];
                    return (
                      <button
                        key={key}
                        type="button"
                        onClick={() => {
                          setGuidedChain(key);
                          if (key !== 'base' && guidedAsset === 'WLOVE') setGuidedAsset('NATIVE');
                        }}
                        className={`rounded-xl border px-2 py-2 text-xs font-medium transition cursor-pointer ${
                          guidedChain === key
                            ? 'border-white/30 bg-white/[0.1] text-white'
                            : 'border-white/[0.08] bg-[#0a0a0d] text-zinc-400 hover:text-zinc-100'
                        }`}
                      >
                        {item.shortName}
                      </button>
                    );
                  })}
                </div>

                <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                  <div className="rounded-xl border border-white/[0.08] bg-[#0a0a0d] p-3">
                    <div className="flex items-center justify-between">
                      <span className="text-[11px] text-zinc-500">Asset &amp; Amount</span>
                      <div className="inline-flex gap-1">
                        <button
                          type="button"
                          onClick={() => {
                            setGuidedAsset('NATIVE');
                            setGuidedAmount(guidedChain === 'polygon' ? '5' : '0.002');
                          }}
                          className={`rounded px-1.5 py-0.5 font-mono text-[10px] cursor-pointer ${
                            guidedAsset === 'NATIVE'
                              ? 'bg-white/[0.15] text-white'
                              : 'text-zinc-500 hover:text-zinc-200'
                          }`}
                        >
                          {guidedCfg.nativeSymbol}
                        </button>
                        <button
                          type="button"
                          onClick={() => {
                            setGuidedAsset('USDC');
                            setGuidedAmount('5');
                          }}
                          className={`rounded px-1.5 py-0.5 font-mono text-[10px] cursor-pointer ${
                            guidedAsset === 'USDC'
                              ? 'bg-white/[0.15] text-white'
                              : 'text-zinc-500 hover:text-zinc-200'
                          }`}
                        >
                          USDC
                        </button>
                        {guidedChain === 'base' && (
                          <button
                            type="button"
                            onClick={() => {
                              setGuidedAsset('WLOVE');
                              setGuidedAmount('100000');
                            }}
                            className={`rounded px-1.5 py-0.5 font-mono text-[10px] cursor-pointer ${
                              guidedAsset === 'WLOVE'
                                ? 'bg-white/[0.15] text-white'
                                : 'text-zinc-500 hover:text-zinc-200'
                            }`}
                          >
                            wLOVE
                          </button>
                        )}
                      </div>
                    </div>
                    <input
                      type="text"
                      inputMode="decimal"
                      value={guidedAmount}
                      onChange={(e) => setGuidedAmount(e.target.value)}
                      placeholder="0.002"
                      className="mt-2 w-full bg-transparent font-mono text-xl font-medium text-zinc-100 focus:outline-none tabular-nums"
                    />
                  </div>

                  <div className="rounded-xl border border-white/[0.08] bg-[#0a0a0d] p-3">
                    <div className="flex items-center justify-between">
                      <span className="text-[11px] text-zinc-500">Refund Timeout</span>
                      <span className="font-mono text-[11px] text-zinc-300">
                        {guidedTimeoutHours} hours
                      </span>
                    </div>
                    <div className="mt-3 flex gap-1.5">
                      {[2, 6, 12, 24].map((h) => (
                        <button
                          key={h}
                          type="button"
                          onClick={() => setGuidedTimeoutHours(h)}
                          className={`flex-1 rounded-lg border py-1.5 font-mono text-[11px] transition cursor-pointer ${
                            guidedTimeoutHours === h
                              ? 'border-white/30 bg-white/[0.1] text-white'
                              : 'border-white/[0.08] text-zinc-400 hover:text-zinc-100'
                          }`}
                        >
                          {h}h
                        </button>
                      ))}
                    </div>
                  </div>
                </div>

                <div>
                  <label className="mb-1 block text-[11px] text-zinc-400">
                    Counterparty Maker EVM Address (receives locked {guidedSymbol} when they reveal the secret on L1)
                  </label>
                  <input
                    type="text"
                    value={guidedCounterparty}
                    onChange={(e) => setGuidedCounterparty(e.target.value)}
                    placeholder="0x… maker address"
                    className="w-full rounded-xl border border-white/[0.1] bg-[#0a0a0d] px-3 py-2.5 font-mono text-xs text-zinc-100 placeholder:text-zinc-600 focus:border-white/30 focus:outline-none"
                  />
                </div>

                {/* Auto-Generated Secret Box */}
                <div className="rounded-xl border border-white/[0.08] bg-[#0a0a0d] p-3.5">
                  <div className="flex flex-wrap items-center justify-between gap-2">
                    <span className="text-[11px] font-medium text-zinc-300">
                      Local SHA-256 Swap Secret (auto-created if blank)
                    </span>
                    <div className="flex items-center gap-1.5">
                      <button
                        type="button"
                        onClick={() => void handleGenerateLocalSecret()}
                        className="inline-flex items-center gap-1 rounded-lg border border-emerald-500/35 bg-emerald-500/10 px-2.5 py-1 font-mono text-[10px] font-medium text-emerald-300 hover:bg-emerald-500/20 cursor-pointer"
                      >
                        <KeyRound className="h-3 w-3" />
                        <span>{paymentHash ? 'Regenerate' : 'Generate Secret'}</span>
                      </button>
                      {localPreimage && (
                        <button
                          type="button"
                          onClick={handleDownloadSecretReceipt}
                          className="inline-flex items-center gap-1 rounded-lg border border-white/[0.12] bg-white/[0.05] px-2.5 py-1 text-[10px] font-medium text-zinc-200 hover:bg-white/[0.1] cursor-pointer"
                        >
                          <Download className="h-3 w-3" />
                          <span>Save Receipt</span>
                        </button>
                      )}
                    </div>
                  </div>
                  {paymentHash ? (
                    <div className="mt-2 space-y-1 font-mono text-[10px]">
                      <div className="flex items-center justify-between gap-2 text-zinc-400">
                        <span className="truncate">Hash: {paymentHash}</span>
                        <CopyButton value={paymentHash} />
                      </div>
                      {localPreimage && (
                        <div className="truncate text-amber-300/90">
                          Private Preimage: {truncateHash(localPreimage, 14, 10)} (saved locally only)
                        </div>
                      )}
                    </div>
                  ) : (
                    <p className="mt-1.5 text-[11px] text-zinc-500">
                      Click Lock below and we&apos;ll automatically generate a fresh 32-byte secret right inside your browser.
                    </p>
                  )}
                </div>

                <button
                  type="button"
                  disabled={lockingHtlc !== null}
                  onClick={() => void handleLockEvmHtlcDirect()}
                  className="flex w-full items-center justify-center gap-2 rounded-xl bg-zinc-50 px-4 py-3 text-[13px] font-semibold text-zinc-950 transition hover:bg-white cursor-pointer disabled:opacity-50"
                >
                  {lockingHtlc ? (
                    <>
                      <Loader2 className="h-4 w-4 animate-spin" />
                      <span>{lockingHtlc}</span>
                    </>
                  ) : !evmAddress ? (
                    <>
                      <Wallet className="h-4 w-4" />
                      <span>Connect Wallet to Lock HTLC</span>
                    </>
                  ) : evmChainId !== guidedCfg.chainId ? (
                    <span>Switch Wallet to {guidedCfg.name}</span>
                  ) : (
                    <span>
                      Lock {guidedAmount || '0'} {guidedSymbol} in {guidedCfg.shortName} HTLC
                    </span>
                  )}
                </button>

                {htlcLockTx && (
                  <div className="flex flex-wrap items-center justify-between gap-2 rounded-xl border border-emerald-500/30 bg-emerald-500/10 px-3.5 py-2.5 text-xs text-emerald-200">
                    <span className="inline-flex items-center gap-1.5">
                      <CheckCircle2 className="h-3.5 w-3.5 text-emerald-400" />
                      <span>HTLC locked on {guidedCfg.shortName}!</span>
                    </span>
                    <div className="flex items-center gap-3">
                      {localPreimage && (
                        <button
                          type="button"
                          onClick={handleDownloadSecretReceipt}
                          className="font-mono text-[11px] underline cursor-pointer"
                        >
                          Download Secret JSON
                        </button>
                      )}
                      <a
                        href={`${guidedCfg.explorerUrl}/tx/${htlcLockTx}`}
                        target="_blank"
                        rel="noreferrer"
                        className="inline-flex items-center gap-1 font-mono text-[11px] underline"
                      >
                        <span>View TX</span>
                        <ArrowUpRight className="h-3 w-3" />
                      </a>
                    </div>
                  </div>
                )}
              </div>
            </div>

            {/* Multi-Chain Wallet Auto-Fill & Native L1 Shortcut */}
            <div className="space-y-4 lg:col-span-5">
              <div className="surface-card rounded-2xl p-5">
                <h3 className="text-[14px] font-semibold text-zinc-100">
                  Connected Bridge Wallets
                </h3>
                <p className="mt-1 text-xs text-zinc-500">
                  Connect once — every address auto-fills below so you don&apos;t have to hunt for pubkeys.
                </p>
                <div className="mt-4 space-y-2 text-xs">
                  <div className="flex items-center justify-between rounded-xl border border-white/[0.07] bg-[#0a0a0d] px-3.5 py-2.5">
                    <div>
                      <div className="font-medium text-zinc-200">EVM (Base / Arb / OP / Pol)</div>
                      <div className="font-mono text-[10px] text-zinc-500">
                        {evmAddress ? truncateHash(evmAddress, 8, 6) : 'Not connected'}
                      </div>
                    </div>
                    <button
                      type="button"
                      onClick={openModal}
                      className="rounded-lg border border-white/[0.12] px-2.5 py-1 text-[11px] text-zinc-200 hover:bg-white/[0.06] cursor-pointer"
                    >
                      {evmAddress ? 'Manage' : 'Connect'}
                    </button>
                  </div>

                  <div className="flex items-center justify-between rounded-xl border border-white/[0.07] bg-[#0a0a0d] px-3.5 py-2.5">
                    <div>
                      <div className="font-medium text-zinc-200">Solana (Phantom / Solflare)</div>
                      <div className="font-mono text-[10px] text-zinc-500">
                        {solanaAddress ? truncateHash(solanaAddress, 6, 5) : 'Not connected'}
                      </div>
                    </div>
                    <button
                      type="button"
                      onClick={() => void connectSolana()}
                      className="rounded-lg border border-white/[0.12] px-2.5 py-1 text-[11px] text-zinc-200 hover:bg-white/[0.06] cursor-pointer"
                    >
                      {solanaAddress ? 'Connected' : 'Connect'}
                    </button>
                  </div>

                  <div className="flex items-center justify-between rounded-xl border border-white/[0.07] bg-[#0a0a0d] px-3.5 py-2.5">
                    <div>
                      <div className="font-medium text-zinc-200">Bitcoin (UniSat / Leather)</div>
                      <div className="font-mono text-[10px] text-zinc-500">
                        {btcAddress ? truncateHash(btcAddress, 8, 6) : 'Not connected'}
                      </div>
                    </div>
                    <button
                      type="button"
                      onClick={() => void connectBtc()}
                      className="rounded-lg border border-white/[0.12] px-2.5 py-1 text-[11px] text-zinc-200 hover:bg-white/[0.06] cursor-pointer"
                    >
                      {btcAddress ? 'Connected' : 'Connect'}
                    </button>
                  </div>
                </div>
              </div>

              <div className="surface-card rounded-2xl p-5 text-xs">
                <div className="font-medium text-zinc-100">Need a fast swap instead?</div>
                <p className="mt-1 text-zinc-400">
                  If you don&apos;t have a peer-to-peer L1 maker lined up yet, use the Instant Swap tab to trade ETH or USDC directly into Base wLOVE in 1 click.
                </p>
                <button
                  type="button"
                  onClick={() => setSection('swap')}
                  className="mt-3 inline-flex items-center gap-1.5 rounded-lg border border-white/[0.14] bg-white/[0.04] px-3 py-1.5 text-xs font-medium text-zinc-100 hover:bg-white/[0.08] cursor-pointer"
                >
                  <span>Go to Instant Swap</span>
                  <ArrowUpRight className="h-3.5 w-3.5" />
                </button>
              </div>
            </div>
          </div>

          {/* Collapsible Advanced Power-User HTLC Compiler & Verifier */}
          <div className="rounded-2xl border border-white/[0.08] bg-[#0f0f12]">
            <button
              type="button"
              onClick={() => setShowAdvanced((v) => !v)}
              className="flex w-full items-center justify-between px-5 py-4 text-left cursor-pointer"
            >
              <div>
                <div className="text-xs font-semibold text-zinc-200">
                  Advanced Developer Tools · Raw BTC / Lightning / Solana / EVM HTLC Compiler &amp; Dual-Chain Verifier
                </div>
                <div className="mt-0.5 text-[11px] text-zinc-500">
                  Compile BIP-199 SegWit P2WSH scripts, decode BOLT-11 invoices, or cross-check live L1 + mempool.space locks.
                </div>
              </div>
              {showAdvanced ? (
                <ChevronUp className="h-4 w-4 text-zinc-400 shrink-0" />
              ) : (
                <ChevronDown className="h-4 w-4 text-zinc-400 shrink-0" />
              )}
            </button>

            {showAdvanced && (
              <div className="grid grid-cols-1 gap-6 border-t border-white/[0.07] p-5 xl:grid-cols-12">
                <div className="xl:col-span-7">
                  <Panel
                    title="Raw Multi-Chain HTLC Compiler (BTC · Lightning · Solana · EVM)"
                    subtitle="Canonical SHA-256 preimage contracts matching newsatoshi lock --hash"
                    icon={<ArrowLeftRight className="h-4 w-4" />}
                    accent="emerald"
                  >
                    <form onSubmit={handleCompile} className="space-y-4">
                      <div className="grid grid-cols-2 gap-1.5 sm:grid-cols-4">
                        {(
                          [
                            ['evm_htlc', 'Base / EVM'],
                            ['btc_onchain', 'BTC P2WSH'],
                            ['lightning_bolt11', 'Lightning'],
                            ['solana_htlc', 'Solana'],
                          ] as const
                        ).map(([r, label]) => (
                          <button
                            key={r}
                            type="button"
                            onClick={() => setRail(r)}
                            className={`rounded-lg border px-2.5 py-2 text-xs font-medium transition cursor-pointer ${
                              rail === r
                                ? 'border-emerald-500/60 bg-emerald-500/15 text-emerald-200'
                                : 'border-slate-800 bg-slate-950/70 text-slate-400 hover:text-slate-200'
                            }`}
                          >
                            {label}
                          </button>
                        ))}
                      </div>

                      <div className="grid grid-cols-3 gap-1.5">
                        <select
                          value={direction}
                          onChange={(e) => setDirection(e.target.value as 'btc_to_love' | 'love_to_btc')}
                          className="rounded-lg border border-slate-800 bg-slate-950 px-2 py-2 font-mono text-[11px] text-slate-200"
                        >
                          <option value="btc_to_love">Bridge In → LOVE</option>
                          <option value="love_to_btc">LOVE → Bridge Out</option>
                        </select>
                        <select
                          value={loveNet}
                          onChange={(e) => setLoveNet(e.target.value as 'mainnet' | 'testnet')}
                          className="rounded-lg border border-slate-800 bg-slate-950 px-2 py-2 font-mono text-[11px] text-slate-200"
                        >
                          <option value="mainnet">LOVE Mainnet</option>
                          <option value="testnet">LOVE Testnet</option>
                        </select>
                        {rail === 'solana_htlc' ? (
                          <select
                            value={solCluster}
                            onChange={(e) => setSolCluster(e.target.value)}
                            className="rounded-lg border border-slate-800 bg-slate-950 px-2 py-2 font-mono text-[11px] text-slate-200"
                          >
                            <option value="mainnet-beta">Solana Mainnet</option>
                            <option value="devnet">Solana Devnet</option>
                          </select>
                        ) : rail === 'evm_htlc' ? (
                          <select
                            value={evmChain}
                            onChange={(e) => setEvmChain(e.target.value)}
                            className="rounded-lg border border-slate-800 bg-slate-950 px-2 py-2 font-mono text-[11px] text-slate-200"
                          >
                            <option value="base">Base (8453)</option>
                            <option value="arbitrum">Arbitrum (42161)</option>
                            <option value="optimism">Optimism (10)</option>
                            <option value="polygon">Polygon (137)</option>
                          </select>
                        ) : (
                          <select
                            value={btcNet}
                            onChange={(e) =>
                              setBtcNet(e.target.value as 'mainnet' | 'testnet' | 'signet' | 'regtest')
                            }
                            className="rounded-lg border border-slate-800 bg-slate-950 px-2 py-2 font-mono text-[11px] text-slate-200"
                          >
                            <option value="mainnet">BTC Mainnet</option>
                            <option value="testnet">BTC Testnet</option>
                            <option value="signet">BTC Signet</option>
                            <option value="regtest">BTC Regtest</option>
                          </select>
                        )}
                      </div>

                      {rail !== 'lightning_bolt11' ? (
                        <div className="space-y-3">
                          <div>
                            <div className="mb-1 flex items-center justify-between">
                              <label className="font-mono text-[10px] uppercase tracking-wider text-slate-400">
                                SHA-256 Payment Hash (64 hex chars)
                              </label>
                              <button
                                type="button"
                                onClick={() => void handleGenerateLocalSecret()}
                                className="inline-flex items-center gap-1 rounded border border-emerald-500/40 bg-emerald-500/10 px-2 py-0.5 font-mono text-[10px] font-medium text-emerald-300 hover:bg-emerald-500/20 cursor-pointer"
                              >
                                <KeyRound className="h-3 w-3" />
                                Generate Local Secret
                              </button>
                            </div>
                            <input
                              type="text"
                              required
                              value={paymentHash}
                              onChange={(e) => setPaymentHash(e.target.value)}
                              placeholder="64-hex SHA-256 hash"
                              className="w-full rounded-lg border border-slate-800 bg-slate-950 px-3 py-2 font-mono text-xs text-slate-100 placeholder:text-slate-600"
                            />
                          </div>

                          {rail === 'btc_onchain' && (
                            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                              <input
                                type="text"
                                required
                                value={btcClaimPk}
                                onChange={(e) => setBtcClaimPk(e.target.value)}
                                placeholder="BTC Claim Pubkey (02…/03… 66 hex)"
                                className="w-full rounded-lg border border-slate-800 bg-slate-950 px-3 py-2 font-mono text-xs text-slate-100 placeholder:text-slate-600"
                              />
                              <input
                                type="text"
                                required
                                value={btcRefundPk}
                                onChange={(e) => setBtcRefundPk(e.target.value)}
                                placeholder="BTC Refund Pubkey (auto-filled from UniSat)"
                                className="w-full rounded-lg border border-slate-800 bg-slate-950 px-3 py-2 font-mono text-xs text-slate-100 placeholder:text-slate-600"
                              />
                            </div>
                          )}

                          {rail === 'solana_htlc' && (
                            <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
                              <select
                                value={sourceAsset}
                                onChange={(e) => setSourceAsset(e.target.value)}
                                className="w-full rounded-lg border border-slate-800 bg-slate-950 px-3 py-2 font-mono text-xs text-slate-100"
                              >
                                <option value="USDC">USDC (SPL)</option>
                                <option value="SOL">Native SOL</option>
                              </select>
                              <input
                                type="text"
                                required
                                value={solSender}
                                onChange={(e) => setSolSender(e.target.value)}
                                placeholder="Solana sender (auto-filled from Phantom)"
                                className="w-full rounded-lg border border-slate-800 bg-slate-950 px-3 py-2 font-mono text-xs text-slate-100 placeholder:text-slate-600"
                              />
                              <input
                                type="text"
                                required
                                value={solRecipient}
                                onChange={(e) => setSolRecipient(e.target.value)}
                                placeholder="Counterparty Base58 pubkey"
                                className="w-full rounded-lg border border-slate-800 bg-slate-950 px-3 py-2 font-mono text-xs text-slate-100 placeholder:text-slate-600"
                              />
                            </div>
                          )}

                          {rail === 'evm_htlc' && (
                            <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
                              <select
                                value={sourceAsset}
                                onChange={(e) => setSourceAsset(e.target.value)}
                                className="w-full rounded-lg border border-slate-800 bg-slate-950 px-3 py-2 font-mono text-xs text-slate-100"
                              >
                                <option value="USDC">USDC (ERC-20)</option>
                                <option value="ETH">Native ETH</option>
                              </select>
                              <input
                                type="text"
                                required
                                value={evmRecipient}
                                onChange={(e) => setEvmRecipient(e.target.value)}
                                placeholder="0x… counterparty address"
                                className="sm:col-span-2 w-full rounded-lg border border-slate-800 bg-slate-950 px-3 py-2 font-mono text-xs text-slate-100 placeholder:text-slate-600"
                              />
                            </div>
                          )}
                        </div>
                      ) : (
                        <textarea
                          required
                          rows={3}
                          value={bolt11Invoice}
                          onChange={(e) => setBolt11Invoice(e.target.value)}
                          placeholder="Paste Lightning BOLT-11 invoice (lnbc… / lntb…)"
                          className="w-full rounded-lg border border-slate-800 bg-slate-950 px-3 py-2 font-mono text-xs text-slate-100 placeholder:text-slate-600"
                        />
                      )}

                      <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
                        <input
                          type="text"
                          required
                          value={amountLove}
                          onChange={(e) => setAmountLove(e.target.value)}
                          placeholder="LOVE amount (e.g. 250)"
                          className="w-full rounded-lg border border-slate-800 bg-slate-950 px-3 py-2 font-mono text-xs text-slate-100 placeholder:text-slate-600"
                        />
                        <input
                          type="number"
                          min={1}
                          required={rail !== 'lightning_bolt11'}
                          value={amountBtcSats}
                          onChange={(e) => setAmountBtcSats(e.target.value)}
                          placeholder={
                            rail === 'lightning_bolt11'
                              ? 'Auto from BOLT-11'
                              : rail === 'solana_htlc' || rail === 'evm_htlc'
                                ? `${sourceAsset} base units`
                                : 'BTC sats'
                          }
                          className="w-full rounded-lg border border-slate-800 bg-slate-950 px-3 py-2 font-mono text-xs text-slate-100 placeholder:text-slate-600"
                        />
                        <input
                          type="number"
                          min={1}
                          max={20160}
                          required
                          value={loveTimeoutBlocks}
                          onChange={(e) => setLoveTimeoutBlocks(e.target.value)}
                          placeholder="LOVE timeout blocks (144)"
                          className="w-full rounded-lg border border-slate-800 bg-slate-950 px-3 py-2 font-mono text-xs text-slate-100"
                        />
                      </div>

                      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                        <input
                          type="text"
                          required
                          value={loveClaimAddr}
                          onChange={(e) => setLoveClaimAddr(e.target.value)}
                          placeholder="Newsatoshi claim address (ns1… / nsd1…)"
                          className="w-full rounded-lg border border-slate-800 bg-slate-950 px-3 py-2 font-mono text-xs text-slate-100 placeholder:text-slate-600"
                        />
                        {rail === 'btc_onchain' ? (
                          <div className="grid grid-cols-2 gap-2">
                            <input
                              type="number"
                              min={1}
                              required
                              value={btcLocktime}
                              onChange={(e) => setBtcLocktime(e.target.value)}
                              placeholder="BTC CLTV height"
                              className="w-full rounded-lg border border-slate-800 bg-slate-950 px-3 py-2 font-mono text-xs text-slate-100 placeholder:text-slate-600"
                            />
                            <input
                              type="number"
                              min={1}
                              value={btcCurrentHeight}
                              onChange={(e) => setBtcCurrentHeight(e.target.value)}
                              placeholder="BTC tip (opt)"
                              className="w-full rounded-lg border border-slate-800 bg-slate-950 px-3 py-2 font-mono text-xs text-slate-100 placeholder:text-slate-600"
                            />
                          </div>
                        ) : (
                          <input
                            type="text"
                            value={makerLabel}
                            onChange={(e) => setMakerLabel(e.target.value)}
                            placeholder="Optional tag"
                            className="w-full rounded-lg border border-slate-800 bg-slate-950 px-3 py-2 font-mono text-xs text-slate-100 placeholder:text-slate-600"
                          />
                        )}
                      </div>

                      {compileError && (
                        <div className="rounded-lg border border-red-500/40 bg-red-500/10 px-3 py-2 text-xs text-red-200">
                          {compileError}
                        </div>
                      )}

                      <div className="flex flex-wrap gap-2">
                        <button
                          type="submit"
                          disabled={compiling}
                          className="inline-flex items-center gap-2 rounded-lg bg-emerald-500 px-4 py-2 text-xs font-semibold text-slate-950 hover:bg-emerald-400 cursor-pointer disabled:opacity-50"
                        >
                          <ShieldCheck className="h-4 w-4" />
                          {compiling ? 'Compiling…' : 'Compile HTLC Contract'}
                        </button>
                        {hasOperatorToken && (
                          <button
                            type="button"
                            disabled={publishing}
                            onClick={() => void handlePublish()}
                            className="inline-flex items-center gap-2 rounded-lg border border-sky-500/50 bg-sky-500/15 px-3.5 py-2 text-xs font-semibold text-sky-200 hover:bg-sky-500/25 cursor-pointer disabled:opacity-50"
                          >
                            {publishing ? 'Publishing…' : 'Publish to Board'}
                          </button>
                        )}
                      </div>
                    </form>

                    {compiled && (
                      <div className="mt-4 space-y-2.5 rounded-xl border border-emerald-500/35 bg-[#04080f] p-3.5 text-xs">
                        <div className="flex items-center justify-between gap-2 font-mono text-emerald-200">
                          <span>
                            {compiled.swap_id} · {compiled.amount_love} LOVE
                          </span>
                          <span className="text-[10px]">
                            {compiled.timelock_safe ? 'TIMELOCK SAFE' : 'CHECK TIMELOCK'}
                          </span>
                        </div>
                        {compiled.btc_htlc && (
                          <div className="flex items-center justify-between font-mono text-[11px]">
                            <span className="text-slate-400">BTC P2WSH</span>
                            <HashPill hash={compiled.btc_htlc.btc_p2wsh_address} full />
                          </div>
                        )}
                        <div className="rounded-lg border border-slate-800 bg-slate-950 p-2.5">
                          <div className="mb-1 flex items-center justify-between">
                            <span className="font-mono text-[10px] text-emerald-400">
                              L1 Lock Command
                            </span>
                            <CopyButton value={compiled.love_lock_command} label="Copy" />
                          </div>
                          <code className="block break-all font-mono text-[10px] text-emerald-200">
                            {compiled.love_lock_command}
                          </code>
                        </div>
                      </div>
                    )}
                  </Panel>
                </div>

                <div className="space-y-4 xl:col-span-5">
                  <Panel
                    title="Live Dual-Chain HTLC Verifier"
                    subtitle="Checks Newsatoshi L1 + mempool.space live"
                    icon={<SearchCheck className="h-4 w-4" />}
                    accent="sky"
                  >
                    <form onSubmit={handleVerifyBothChains} className="space-y-2.5">
                      <div className="grid grid-cols-3 gap-2">
                        <input
                          type="text"
                          required
                          value={verifyHash}
                          onChange={(e) => setVerifyHash(e.target.value)}
                          placeholder="64-hex payment hash"
                          className="col-span-2 rounded-lg border border-slate-800 bg-slate-950 px-3 py-2 font-mono text-xs text-slate-100 placeholder:text-slate-600"
                        />
                        <select
                          value={verifyLoveNet}
                          onChange={(e) => setVerifyLoveNet(e.target.value as 'mainnet' | 'testnet')}
                          className="rounded-lg border border-slate-800 bg-slate-950 px-2 py-2 font-mono text-xs text-slate-200"
                        >
                          <option value="mainnet">Mainnet</option>
                          <option value="testnet">Testnet</option>
                        </select>
                      </div>
                      <input
                        type="text"
                        value={verifyBtcAddr}
                        onChange={(e) => setVerifyBtcAddr(e.target.value)}
                        placeholder="Optional BTC P2WSH (bc1q…)"
                        className="w-full rounded-lg border border-slate-800 bg-slate-950 px-3 py-2 font-mono text-xs text-slate-100 placeholder:text-slate-600"
                      />
                      <input
                        type="text"
                        value={verifyPreimage}
                        onChange={(e) => setVerifyPreimage(e.target.value)}
                        placeholder="Optional 64-hex preimage"
                        className="w-full rounded-lg border border-slate-800 bg-slate-950 px-3 py-2 font-mono text-xs text-slate-100 placeholder:text-slate-600"
                      />
                      <button
                        type="submit"
                        disabled={verifying}
                        className="inline-flex w-full items-center justify-center gap-2 rounded-lg bg-sky-500 px-4 py-2 text-xs font-semibold text-slate-950 hover:bg-sky-400 cursor-pointer disabled:opacity-50"
                      >
                        <Radio className="h-4 w-4" />
                        {verifying ? 'Checking Live Chains…' : 'Verify Both Chains Live'}
                      </button>
                      {verifyError && (
                        <div className="rounded-lg border border-red-500/40 bg-red-500/10 px-3 py-2 text-xs text-red-200">
                          {verifyError}
                        </div>
                      )}
                      {verifyResult && (
                        <div className="space-y-1.5 rounded-lg border border-slate-800 bg-[#04080f] p-3 font-mono text-[11px]">
                          <div className="flex items-center justify-between">
                            <span className="text-slate-400">L1 ({verifyResult.love_network})</span>
                            <span className={verifyResult.love_confirmed ? 'text-emerald-300' : 'text-slate-300'}>
                              {verifyResult.love_confirmed
                                ? `CONFIRMED #${verifyResult.love_lock?.at.toLocaleString()}`
                                : verifyResult.love_found
                                  ? 'MEMPOOL'
                                  : 'NOT FOUND'}
                            </span>
                          </div>
                          {verifyResult.btc_address && (
                            <div className="flex items-center justify-between">
                              <span className="text-slate-400">Bitcoin</span>
                              <span className="text-slate-200">
                                {verifyResult.btc_found
                                  ? `${verifyResult.btc_total_sats.toLocaleString()} sats`
                                  : '0 UTXOs'}
                              </span>
                            </div>
                          )}
                        </div>
                      )}
                    </form>
                  </Panel>

                  <Panel
                    title="Persisted Swap Board"
                    subtitle="Strictly real persisted HTLC swap intents"
                    icon={<AlertTriangle className="h-4 w-4 text-amber-400" />}
                    badge={<span className="font-mono text-[11px] text-slate-300">{swapOffers.length} active</span>}
                  >
                    {swapOffers.length === 0 ? (
                      <div className="rounded-lg border border-dashed border-slate-800 px-3 py-4 text-center text-xs text-slate-400">
                        No open peer-to-peer swap intents yet. Use <strong>Instant Swap</strong> for immediate Base liquidity.
                      </div>
                    ) : (
                      <div className="space-y-2">
                        {swapOffers.map((offer) => (
                          <div
                            key={offer.swap_id}
                            className="rounded-lg border border-slate-800 bg-slate-950/80 p-2.5 text-xs"
                          >
                            <div className="flex items-center justify-between">
                              <span className="font-mono font-semibold text-slate-100">
                                {offer.amount_love} LOVE ↔ {offer.amount_btc_sats.toLocaleString()} sats
                              </span>
                              <span className="font-mono text-[10px] text-slate-400">{offer.rail}</span>
                            </div>
                          </div>
                        ))}
                      </div>
                    )}
                  </Panel>
                </div>
              </div>
            )}
          </div>
        </div>
      )}

      {/* ═══════════════════════════════════════════════════════════════════════ */}
      {/* TAB 4: EASY SETUP & AUXPOW MERGE-MINING                                */}
      {/* ═══════════════════════════════════════════════════════════════════════ */}
      {section === 'setup' && (
        <div className="space-y-6">
          {/* Top 2 Easy Setup Cards: Humans & AI Agents */}
          <div className="grid grid-cols-1 gap-6 lg:grid-cols-2">
            {/* Card 1: Wallet & CLI Setup */}
            <div className="surface-card rounded-2xl p-6 space-y-4">
              <div className="flex items-center justify-between">
                <span className="font-mono text-[10px] uppercase tracking-[0.12em] text-emerald-400">
                  Step-by-Step · 30 Seconds
                </span>
                <span className="rounded-full border border-white/[0.08] px-2.5 py-0.5 text-[10px] text-zinc-400">
                  Humans &amp; Wallets
                </span>
              </div>
              <h3 className="font-display text-2xl text-zinc-100">
                Set up your wallets with zero friction.
              </h3>

              <div className="space-y-2.5 text-xs">
                <div className="flex items-center justify-between rounded-xl border border-white/[0.07] bg-[#0a0a0d] p-3.5">
                  <div>
                    <div className="font-medium text-zinc-100">1. Connect EVM Wallet &amp; Add wLOVE</div>
                    <div className="mt-0.5 text-[11px] text-zinc-500">
                      {evmAddress
                        ? `Connected ${truncateHash(evmAddress, 6, 4)}`
                        : 'MetaMask, Coinbase Wallet, Rabby, or Phantom'}
                    </div>
                  </div>
                  <div className="flex gap-1.5">
                    <button
                      type="button"
                      onClick={openModal}
                      className="rounded-lg bg-zinc-100 px-3 py-1.5 text-[11px] font-semibold text-zinc-950 hover:bg-white cursor-pointer"
                    >
                      {evmAddress ? 'Wallet' : 'Connect'}
                    </button>
                    {evmAddress && (
                      <button
                        type="button"
                        onClick={() => void addWloveToWallet()}
                        className="rounded-lg border border-emerald-500/35 bg-emerald-500/10 px-2.5 py-1.5 text-[11px] font-medium text-emerald-300 hover:bg-emerald-500/20 cursor-pointer"
                      >
                        + wLOVE
                      </button>
                    )}
                  </div>
                </div>

                <div className="rounded-xl border border-white/[0.07] bg-[#0a0a0d] p-3.5 space-y-2">
                  <div className="flex items-center justify-between">
                    <div>
                      <div className="font-medium text-zinc-100">2. Native Shielded L1 Wallet (LOVE)</div>
                      <div className="mt-0.5 text-[11px] text-zinc-500">
                        Runs 100% in your browser via WASM — no install required.
                      </div>
                    </div>
                    <a
                      href="https://newsatoshi.org/wallet/"
                      target="_blank"
                      rel="noreferrer"
                      className="inline-flex shrink-0 items-center gap-1 rounded-lg border border-white/[0.12] px-2.5 py-1.5 text-[11px] font-medium text-zinc-200 hover:bg-white/[0.06]"
                    >
                      <span>Open Wallet</span>
                      <ArrowUpRight className="h-3 w-3" />
                    </a>
                  </div>
                  <div className="flex gap-2">
                    <input
                      type="text"
                      value={loveDraft}
                      onChange={(e) => setLoveDraft(e.target.value)}
                      placeholder="Paste ns1… address to save & auto-fill across the app"
                      className="flex-1 rounded-lg border border-white/[0.08] bg-[#0f0f12] px-2.5 py-1.5 font-mono text-[11px] text-zinc-200 placeholder:text-zinc-600 focus:outline-none"
                    />
                    <button
                      type="button"
                      onClick={() => {
                        setLoveAddress(loveDraft);
                        onNotice('success', loveDraft.trim() ? 'Saved L1 address' : 'Cleared L1 address');
                      }}
                      className="rounded-lg border border-white/[0.12] px-3 py-1.5 text-[11px] text-zinc-200 hover:bg-white/[0.06] cursor-pointer"
                    >
                      Save
                    </button>
                  </div>
                </div>

                <div className="rounded-xl border border-white/[0.07] bg-[#0a0a0d] p-3.5">
                  <div className="mb-1.5 flex items-center justify-between">
                    <span className="font-medium text-zinc-100">3. Optional 1-Line CLI Installer</span>
                    <CopyButton value={cliInstallCmd} label="Copy" />
                  </div>
                  <code className="block overflow-x-auto font-mono text-[11px] text-emerald-300">
                    {cliInstallCmd}
                  </code>
                </div>
              </div>
            </div>

            {/* Card 2: 10-Second AI Agent MCP Setup + Live Test */}
            <div className="surface-card rounded-2xl p-6 space-y-4">
              <div className="flex items-center justify-between">
                <span className="font-mono text-[10px] uppercase tracking-[0.12em] text-zinc-400">
                  Zero API Keys · Official MCP &amp; Glama
                </span>
                <span className="rounded-full border border-emerald-500/30 bg-emerald-500/10 px-2.5 py-0.5 font-mono text-[10px] text-emerald-300">
                  19 Live Tools
                </span>
              </div>
              <h3 className="font-display text-2xl text-zinc-100">
                Connect Claude, Cursor, or any AI agent.
              </h3>
              <p className="text-xs leading-relaxed text-zinc-400">
                Drop this snippet into your MCP config for instant zero-API-key{' '}
                <strong className="text-zinc-200">EVM contract security audits</strong>,{' '}
                <strong className="text-zinc-200">Bitcoin mempool fees</strong>,{' '}
                <strong className="text-zinc-200">Lightning BOLT-11 decoding</strong>,{' '}
                <strong className="text-zinc-200">Base Uniswap V3 quotes</strong>, tx finality checks, and Newsatoshi LOVE rails.
              </p>

              <div className="rounded-xl border border-white/[0.08] bg-[#0a0a0d] p-3.5">
                <div className="mb-2 flex items-center justify-between">
                  <span className="font-mono text-[10px] uppercase text-zinc-500">
                    mcp.json · Streamable HTTP
                  </span>
                  <CopyButton value={MCP_SNIPPET} label="Copy JSON" />
                </div>
                <pre className="overflow-x-auto font-mono text-[11.5px] leading-relaxed text-zinc-200">
                  {MCP_SNIPPET}
                </pre>
              </div>

              <div className="flex flex-wrap items-center gap-3">
                <button
                  type="button"
                  disabled={mcpTestBusy}
                  onClick={() => void handleTestLiveMcp()}
                  className="inline-flex items-center gap-2 rounded-xl bg-zinc-100 px-4 py-2 text-xs font-semibold text-zinc-950 hover:bg-white cursor-pointer disabled:opacity-50"
                >
                  {mcpTestBusy ? (
                    <Loader2 className="h-3.5 w-3.5 animate-spin" />
                  ) : (
                    <Zap className="h-3.5 w-3.5" />
                  )}
                  <span>{mcpTestBusy ? 'Pinging /mcp…' : 'Test Live MCP Endpoint'}</span>
                </button>

                {mcpTestResult && (
                  <span
                    className={`font-mono text-xs ${
                      mcpTestResult.ok ? 'text-emerald-300' : 'text-red-300'
                    }`}
                  >
                    {mcpTestResult.ok
                      ? `✓ ${mcpTestResult.toolCount} tools online (${mcpTestResult.latencyMs}ms)`
                      : `Error: ${mcpTestResult.error}`}
                  </span>
                )}
              </div>
            </div>
          </div>

          {/* Public Bitaxe / ASIC Solo-Mining Pool (0% Fee · Direct Coinbase Payout) */}
          <div className="surface-card rounded-2xl p-6 space-y-4">
            <div className="flex flex-wrap items-center justify-between gap-3">
              <div>
                <div className="flex items-center gap-2">
                  <span
                    className={`h-2 w-2 rounded-full ${
                      poolStatus?.online ? 'bg-emerald-400' : 'bg-amber-400'
                    }`}
                  />
                  <span className="font-mono text-[10px] uppercase tracking-[0.12em] text-amber-300">
                    Live Public Bitaxe / ASIC Solo Pool · 0% Fee · Direct L1 Payout
                  </span>
                </div>
                <h3 className="font-display mt-1.5 text-2xl text-zinc-100 sm:text-3xl">
                  Point your Bitaxe or NerdQAxe in 10 seconds — no Docker required.
                </h3>
                <p className="mt-1 max-w-2xl text-xs text-zinc-400">
                  Paste our public Stratum v1 URL into AxeOS or any SHA-256 miner and put your own{' '}
                  <code className="font-mono text-zinc-200">ns1…</code> address as the Username. Winning blocks pay{' '}
                  <strong className="text-zinc-200">100% of the 50 LOVE reward directly to your address</strong> on-chain.
                </p>
              </div>
              <a
                href="https://pool.newsatoshi.org/status"
                target="_blank"
                rel="noreferrer"
                className="inline-flex items-center gap-1 rounded-full border border-white/[0.1] bg-white/[0.03] px-3 py-1.5 font-mono text-[11px] text-zinc-300 hover:border-white/[0.22] hover:text-white"
              >
                <span>Live Pool JSON</span>
                <ArrowUpRight className="h-3 w-3" />
              </a>
            </div>

            {/* 4 Live Pool Metrics (Strict Zero-Fake-Data) */}
            <div className="grid grid-cols-2 gap-2.5 sm:grid-cols-4">
              <div className="rounded-xl border border-white/[0.07] bg-[#0a0a0d] p-3">
                <div className="text-[10px] uppercase tracking-wider text-zinc-500">Pool Status</div>
                <div className="mt-1 font-mono text-sm font-semibold text-emerald-300">
                  {poolStatus?.online
                    ? poolStatus.current_template?.aux_height
                      ? `Online · #${poolStatus.current_template.aux_height.toLocaleString()}`
                      : 'Online · Syncing'
                    : 'Connecting…'}
                </div>
              </div>
              <div className="rounded-xl border border-white/[0.07] bg-[#0a0a0d] p-3">
                <div className="text-[10px] uppercase tracking-wider text-zinc-500">Connected Miners</div>
                <div className="mt-1 font-mono text-sm font-semibold text-zinc-100 tabular-nums">
                  {poolStatus?.online ? (poolStatus.connected_workers ?? 0) : '—'}
                </div>
              </div>
              <div className="rounded-xl border border-white/[0.07] bg-[#0a0a0d] p-3">
                <div className="text-[10px] uppercase tracking-wider text-zinc-500">Accepted Shares</div>
                <div className="mt-1 font-mono text-sm font-semibold text-zinc-100 tabular-nums">
                  {poolStatus?.online ? (poolStatus.shares_accepted ?? 0).toLocaleString() : '—'}
                </div>
              </div>
              <div className="rounded-xl border border-white/[0.07] bg-[#0a0a0d] p-3">
                <div className="text-[10px] uppercase tracking-wider text-zinc-500">Pool Blocks / Fee</div>
                <div className="mt-1 font-mono text-sm font-semibold text-amber-300 tabular-nums">
                  {poolStatus?.online ? `${poolStatus.aux_blocks_accepted ?? 0} blk · 0% fee` : '0% fee'}
                </div>
              </div>
            </div>

            {/* 3-Field AxeOS / Bitaxe Paste Box */}
            <div className="grid grid-cols-1 gap-2.5 sm:grid-cols-3">
              <div className="rounded-xl border border-white/[0.08] bg-[#0a0a0d] p-3">
                <div className="flex items-center justify-between text-[10px] uppercase text-zinc-500">
                  <span>Stratum Host</span>
                  <CopyButton value="pool.newsatoshi.org" label="Copy" />
                </div>
                <div className="mt-1 font-mono text-xs font-medium text-emerald-300">
                  pool.newsatoshi.org
                </div>
              </div>
              <div className="rounded-xl border border-white/[0.08] bg-[#0a0a0d] p-3">
                <div className="flex items-center justify-between text-[10px] uppercase text-zinc-500">
                  <span>Stratum Port</span>
                  <CopyButton value="3333" label="Copy" />
                </div>
                <div className="mt-1 font-mono text-xs font-medium text-zinc-100">
                  3333 <span className="text-zinc-500">(BIP-320 supported)</span>
                </div>
              </div>
              <div className="rounded-xl border border-white/[0.08] bg-[#0a0a0d] p-3">
                <div className="flex items-center justify-between text-[10px] uppercase text-zinc-500">
                  <span>Stratum User (Payout)</span>
                  <CopyButton
                    value={loveAddress ? `${loveAddress}.bitaxe` : 'YOUR_NS1_ADDRESS.bitaxe'}
                    label="Copy"
                  />
                </div>
                <div className="mt-1 truncate font-mono text-xs font-medium text-amber-200">
                  {loveAddress ? `${truncateHash(loveAddress, 8, 5)}.bitaxe` : 'YOUR_NS1_ADDRESS.bitaxe'}
                </div>
              </div>
            </div>
          </div>

          {/* Mining & Hardware Kits */}
          <div className="grid grid-cols-1 gap-6 lg:grid-cols-2">
            <Panel
              title="1-Command AuxPoW Merge-Mining (Zero Extra Watts)"
              subtitle="Mine Newsatoshi Mainnet (chainid 20051) alongside Bitcoin on Bitaxe, NerdQAxe, or Antminer"
              icon={<Pickaxe className="h-4 w-4" />}
              accent="amber"
            >
              <div className="space-y-4">
                {mainnet &&
                  (() => {
                    const ths = Math.max(0.001, Number(minerThs) || 1.2);
                    const hashesPerBlock = Math.max(1e-9, mainnet.difficulty) * 4_294_967_296;
                    const recent = mainnet.recent ?? [];
                    const obsDt =
                      recent.length >= 2 && recent[0].time > recent[recent.length - 1].time
                        ? (recent[0].time - recent[recent.length - 1].time) / (recent.length - 1)
                        : 600;
                    const netHs = hashesPerBlock / Math.max(1, obsDt);
                    const netGhs = netHs / 1e9;
                    const minerHs = ths * 1e12;
                    const postAsertShare = minerHs / (netHs + minerHs);
                    const postAsertLoveDay = postAsertShare * 7_200;
                    return (
                      <div className="space-y-2.5 rounded-xl border border-amber-500/20 bg-amber-950/15 p-3.5">
                        <div className="flex flex-wrap items-center justify-between gap-2">
                          <span className="font-mono text-[10px] uppercase text-amber-300">
                            Live Yield Estimator (#{mainnet.height.toLocaleString()})
                          </span>
                          <span className="font-mono text-[10px] text-zinc-400">
                            Net: {netGhs.toFixed(2)} GH/s
                          </span>
                        </div>
                        <div className="flex flex-wrap items-center gap-1.5">
                          {[
                            { label: 'Bitaxe 1.2T', v: '1.2' },
                            { label: 'NerdQAxe 4.8T', v: '4.8' },
                            { label: 'S19 110T', v: '110' },
                            { label: 'S21 200T', v: '200' },
                          ].map((preset) => (
                            <button
                              key={preset.v}
                              type="button"
                              onClick={() => setMinerThs(preset.v)}
                              className={`rounded-md border px-2 py-0.5 font-mono text-[10px] cursor-pointer ${
                                minerThs === preset.v
                                  ? 'border-amber-400/70 bg-amber-500/20 text-amber-200'
                                  : 'border-white/[0.08] text-zinc-400 hover:text-zinc-200'
                              }`}
                            >
                              {preset.label}
                            </button>
                          ))}
                          <input
                            type="number"
                            min="0.001"
                            step="0.1"
                            value={minerThs}
                            onChange={(e) => setMinerThs(e.target.value)}
                            aria-label="Miner hashrate in TH/s"
                            className="w-18 rounded-md border border-white/[0.1] bg-[#0a0a0d] px-2 py-0.5 font-mono text-[11px] text-zinc-100"
                          />
                          <span className="font-mono text-[10px] text-zinc-400">TH/s</span>
                        </div>
                        <div className="font-mono text-xs font-semibold text-amber-300">
                          ~{postAsertLoveDay.toLocaleString(undefined, { maximumFractionDigits: 1 })}{' '}
                          LOVE/day ({(postAsertShare * 100).toFixed(2)}% of 7,200 LOVE/day emission)
                        </div>
                      </div>
                    );
                  })()}

                <div className="rounded-xl border border-white/[0.08] bg-[#0a0a0d] p-3">
                  <div className="mb-1.5 flex items-center justify-between">
                    <span className="inline-flex items-center gap-1.5 font-mono text-[10px] uppercase text-amber-300">
                      <Terminal className="h-3.5 w-3.5" />
                      1-Command Docker Stratum v1 (:3333)
                    </span>
                    <CopyButton value={dockerQuickstart} label="Copy" />
                  </div>
                  <pre className="overflow-x-auto font-mono text-[11px] leading-relaxed text-zinc-200">
                    {dockerQuickstart}
                  </pre>
                </div>
              </div>
            </Panel>

            <Panel
              title="Field Hardware Energy Meter Signer (SLH-DSA)"
              subtitle="Signs real solar inverter / Shelly EM / Enphase kWh deltas offline with zero disk secrets"
              icon={<Cpu className="h-4 w-4" />}
              accent="sky"
            >
              <div className="space-y-3">
                <div className="rounded-xl border border-white/[0.08] bg-[#0a0a0d] p-3">
                  <div className="mb-1.5 flex items-center justify-between">
                    <span className="font-mono text-[10px] uppercase text-sky-300">
                      Hardware Attestation Daemon
                    </span>
                    <CopyButton value={meterDaemonCmd} label="Copy" />
                  </div>
                  <pre className="overflow-x-auto font-mono text-[10.5px] leading-relaxed text-zinc-200">
                    {meterDaemonCmd}
                  </pre>
                </div>
                <p className="text-[11px] leading-relaxed text-zinc-400">
                  First poll records your meter&apos;s real cumulative baseline with 0 synthetic emissions; every subsequent whole-kWh delta produces an offline NIST FIPS 205 SLH-DSA signature verified into a live Winterfell STARK proof.
                </p>
              </div>
            </Panel>
          </div>
        </div>
      )}
    </div>
  );
}
