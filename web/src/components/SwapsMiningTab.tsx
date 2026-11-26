import { useState } from 'react';
import {
  AlertTriangle,
  ArrowLeftRight,
  CheckCircle2,
  Cpu,
  KeyRound,
  Pickaxe,
  Radio,
  SearchCheck,
  ShieldCheck,
  Terminal,
  Zap,
} from 'lucide-react';
import { actionCompileSwap, actionPublishSwap, actionVerifySwap } from '../api/client';
import type {
  AtomicSwapContract,
  CommonsState,
  CompileSwapPayload,
  DualChainSwapVerification,
  SeedStatusResponse,
} from '../types';
import { CopyButton, HashPill } from './ui/CopyButton';
import { Panel } from './ui/Panel';

interface SwapsMiningTabProps {
  state: CommonsState | null;
  seed: SeedStatusResponse | null;
  hasOperatorToken: boolean;
  onStateUpdated: (next: CommonsState) => void;
  onNotice: (kind: 'success' | 'error', message: string) => void;
}

function bytesToHex(bytes: Uint8Array): string {
  return Array.from(bytes)
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('');
}

export function SwapsMiningTab({
  state,
  seed,
  hasOperatorToken,
  onStateUpdated,
  onNotice,
}: SwapsMiningTabProps) {
  const [section, setSection] = useState<'swaps' | 'mining'>('swaps');

  // Multi-chain Atomic Swap Compiler form state (strictly blank inputs — zero fake keys or orders)
  const [rail, setRail] = useState<'btc_onchain' | 'lightning_bolt11' | 'solana_htlc' | 'evm_htlc'>('btc_onchain');
  const [direction, setDirection] = useState<'btc_to_love' | 'love_to_btc'>('btc_to_love');
  const [loveNet, setLoveNet] = useState<'mainnet' | 'testnet'>('mainnet');
  const [btcNet, setBtcNet] = useState<'mainnet' | 'testnet' | 'signet' | 'regtest'>('mainnet');
  const [localPreimage, setLocalPreimage] = useState<string>('');
  const [paymentHash, setPaymentHash] = useState<string>('');
  const [bolt11Invoice, setBolt11Invoice] = useState<string>('');
  const [amountLove, setAmountLove] = useState<string>('');
  const [amountBtcSats, setAmountBtcSats] = useState<string>('');
  const [loveClaimAddr, setLoveClaimAddr] = useState<string>('');
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

  // Live AuxPoW Merge-Mining Yield Calculator state (100% live Mainnet difficulty math, zero fake USD prices)
  const [minerThs, setMinerThs] = useState<string>('1.2');

  const handleGenerateLocalSecret = async () => {
    try {
      const preimage = new Uint8Array(32);
      window.crypto.getRandomValues(preimage);
      const digest = new Uint8Array(await window.crypto.subtle.digest('SHA-256', preimage));
      const pHex = bytesToHex(preimage);
      const hHex = bytesToHex(digest);
      setLocalPreimage(pHex);
      setPaymentHash(hHex);
      setVerifyHash(hHex);
      onNotice('success', 'Generated 32-byte CSPRNG secret locally (never sent to server)');
    } catch {
      onNotice('error', 'Web Crypto CSPRNG unavailable in this browser context');
    }
  };

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
            : `Checked Newsatoshi ${res.verification.love_network} L1${res.verification.btc_address ? ' + mempool.space' : ''}`
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

  const mainnet = seed?.mainnet_status;
  const swapOffers = state?.swap_offers ?? [];

  const dockerQuickstart = `git clone https://github.com/theguysaccount/newsatoshi-commons.git
cd newsatoshi-commons/auxpow-stratum
export NS_RPC_AUTH="auxpow:$(openssl rand -hex 16)"
# Optional: set your shielded payout address (ns1... or nsd1...)
# export NS_PAYOUT_ADDRESS="ns1..."
docker compose up -d`;

  const meterDaemonCmd = `agent-vault env-run \\
  --map COMMONS_HARDWARE_XMSS_SEED=newsatoshi/hardware-tee-xmss-seed \\
  --map COMMONS_HARDWARE_SLH_SEED=newsatoshi/hardware-tee-slh-seed -- \\
  newsatoshi-hw-signer poll-meter \\
    --keyfile ./hw-signer-state.json --env-seeds \\
    --adapter shelly-em --meter-url http://192.168.1.50/rpc/EMData.GetStatus?id=0 \\
    --soil 92 --love-per-kwh 10 --scheme slh_dsa \\
    --endpoint https://commons.newsatoshi.org/api/telemetry`;

  return (
    <div className="space-y-6">
      {/* Focused 2-Mode Switcher + Compact Rail Status Strip */}
      <div className="flex flex-wrap items-center justify-between gap-4 rounded-2xl border border-white/[0.08] bg-[#0f0f12] px-4 py-3">
        <div className="inline-flex items-center gap-1 rounded-full border border-white/[0.08] bg-[#0a0a0d] p-1">
          <button
            type="button"
            onClick={() => setSection('swaps')}
            className={`rounded-full px-4 py-1.5 text-xs font-medium transition cursor-pointer ${
              section === 'swaps'
                ? 'bg-zinc-100 text-zinc-950'
                : 'text-zinc-400 hover:text-zinc-100'
            }`}
          >
            Atomic Swaps &amp; Verifier
          </button>
          <button
            type="button"
            onClick={() => setSection('mining')}
            className={`rounded-full px-4 py-1.5 text-xs font-medium transition cursor-pointer ${
              section === 'mining'
                ? 'bg-zinc-100 text-zinc-950'
                : 'text-zinc-400 hover:text-zinc-100'
            }`}
          >
            AuxPoW Mining &amp; Hardware
          </button>
        </div>

        <div className="flex flex-wrap items-center gap-x-4 gap-y-1 text-[11px] text-zinc-400">
          <span className="inline-flex items-center gap-1.5">
            <span className="h-1.5 w-1.5 rounded-full bg-emerald-400" />
            <span>4 EVM Mainnets Live</span>
          </span>
          <span className="inline-flex items-center gap-1.5">
            <span className="h-1.5 w-1.5 rounded-full bg-emerald-400" />
            <span>BTC P2WSH Live</span>
          </span>
          <span className="inline-flex items-center gap-1.5">
            <span className="h-1.5 w-1.5 rounded-full bg-emerald-400" />
            <span>AuxPoW 20051 Live</span>
          </span>
          <span className="font-mono text-zinc-500">Lightning &amp; Solana Ready</span>
        </div>
      </div>

      {/* Section A: Trustless BTC <-> LOVE Atomic Swap Compiler & Verifier */}
      {section === 'swaps' && (
      <div className="grid grid-cols-1 gap-6 xl:grid-cols-12">
        <div className="xl:col-span-7">
          <Panel
            title="Multi-Chain Trustless Buy Roads (BTC · Lightning · Solana · Base/EVM ↔ LOVE)"
            subtitle="All 4 rails share the identical SHA-256 HTLC preimage invariant verified by Newsatoshi L1 — zero custodians, zero wrapped tokens"
            icon={<ArrowLeftRight className="h-4 w-4" />}
            accent="emerald"
            badge={
              <span className="rounded-full border border-emerald-500/40 bg-emerald-500/10 px-2.5 py-0.5 font-mono text-[10px] font-semibold text-emerald-300">
                4 TRUSTLESS RAILS · 0 CUSTODY
              </span>
            }
          >
            <form onSubmit={handleCompile} className="space-y-4">
              <div className="space-y-3">
                <div>
                  <label className="mb-1 block font-mono text-[10px] uppercase tracking-wider text-slate-400">
                    Source Buy / Bridge Rail
                  </label>
                  <div className="grid grid-cols-2 gap-1.5 sm:grid-cols-4">
                    <button
                      type="button"
                      onClick={() => setRail('btc_onchain')}
                      className={`rounded-lg border px-2.5 py-2 text-xs font-medium transition cursor-pointer ${
                        rail === 'btc_onchain'
                          ? 'border-emerald-500/60 bg-emerald-500/15 text-emerald-200'
                          : 'border-slate-800 bg-slate-950/70 text-slate-400 hover:text-slate-200'
                      }`}
                    >
                      BTC P2WSH
                    </button>
                    <button
                      type="button"
                      onClick={() => setRail('lightning_bolt11')}
                      className={`inline-flex items-center justify-center gap-1 rounded-lg border px-2.5 py-2 text-xs font-medium transition cursor-pointer ${
                        rail === 'lightning_bolt11'
                          ? 'border-amber-500/60 bg-amber-500/15 text-amber-200'
                          : 'border-slate-800 bg-slate-950/70 text-slate-400 hover:text-slate-200'
                      }`}
                    >
                      <Zap className="h-3.5 w-3.5" />
                      Lightning
                    </button>
                    <button
                      type="button"
                      onClick={() => setRail('solana_htlc')}
                      className={`rounded-lg border px-2.5 py-2 text-xs font-medium transition cursor-pointer ${
                        rail === 'solana_htlc'
                          ? 'border-purple-500/60 bg-purple-500/15 text-purple-200'
                          : 'border-slate-800 bg-slate-950/70 text-slate-400 hover:text-slate-200'
                      }`}
                    >
                      Solana (SOL/USDC)
                    </button>
                    <button
                      type="button"
                      onClick={() => setRail('evm_htlc')}
                      className={`rounded-lg border px-2.5 py-2 text-xs font-medium transition cursor-pointer ${
                        rail === 'evm_htlc'
                          ? 'border-sky-500/60 bg-sky-500/15 text-sky-200'
                          : 'border-slate-800 bg-slate-950/70 text-slate-400 hover:text-slate-200'
                      }`}
                    >
                      Base / EVM
                    </button>
                  </div>
                </div>

                <div>
                  <label className="mb-1 block font-mono text-[10px] uppercase tracking-wider text-slate-400">
                    Swap Direction &amp; Networks
                  </label>
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
                        <option value="ethereum">Ethereum (1)</option>
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
                </div>
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
                      placeholder="64-hex SHA-256 hash of 32-byte swap preimage"
                      className="w-full rounded-lg border border-slate-800 bg-slate-950 px-3 py-2 font-mono text-xs text-slate-100 placeholder:text-slate-600 focus:border-emerald-500 focus:outline-none"
                    />
                    {localPreimage && (
                      <div className="mt-1.5 flex flex-wrap items-center justify-between gap-2 rounded-lg border border-amber-500/35 bg-amber-500/10 px-3 py-2 text-[11px] text-amber-200">
                        <span className="font-mono break-all">
                          <strong className="text-amber-300">LOCAL SECRET PREIMAGE (keep private):</strong>{' '}
                          {localPreimage}
                        </span>
                        <CopyButton value={localPreimage} label="Copy Secret" />
                      </div>
                    )}
                  </div>

                  {rail === 'btc_onchain' && (
                    <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                      <div>
                        <label className="mb-1 block font-mono text-[10px] uppercase tracking-wider text-slate-400">
                          BTC Claim Pubkey (33-byte compressed hex)
                        </label>
                        <input
                          type="text"
                          required
                          value={btcClaimPk}
                          onChange={(e) => setBtcClaimPk(e.target.value)}
                          placeholder="02… or 03… (66 hex chars)"
                          className="w-full rounded-lg border border-slate-800 bg-slate-950 px-3 py-2 font-mono text-xs text-slate-100 placeholder:text-slate-600 focus:border-emerald-500 focus:outline-none"
                        />
                      </div>
                      <div>
                        <label className="mb-1 block font-mono text-[10px] uppercase tracking-wider text-slate-400">
                          BTC Refund Pubkey (33-byte compressed hex)
                        </label>
                        <input
                          type="text"
                          required
                          value={btcRefundPk}
                          onChange={(e) => setBtcRefundPk(e.target.value)}
                          placeholder="02… or 03… (66 hex chars)"
                          className="w-full rounded-lg border border-slate-800 bg-slate-950 px-3 py-2 font-mono text-xs text-slate-100 placeholder:text-slate-600 focus:border-emerald-500 focus:outline-none"
                        />
                      </div>
                    </div>
                  )}

                  {rail === 'solana_htlc' && (
                    <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
                      <div>
                        <label className="mb-1 block font-mono text-[10px] uppercase tracking-wider text-slate-400">
                          Solana Asset
                        </label>
                        <select
                          value={sourceAsset}
                          onChange={(e) => setSourceAsset(e.target.value)}
                          className="w-full rounded-lg border border-slate-800 bg-slate-950 px-3 py-2 font-mono text-xs text-slate-100"
                        >
                          <option value="USDC">USDC (SPL)</option>
                          <option value="SOL">Native SOL</option>
                        </select>
                      </div>
                      <div>
                        <label className="mb-1 block font-mono text-[10px] uppercase tracking-wider text-slate-400">
                          Solana Sender (Base58)
                        </label>
                        <input
                          type="text"
                          required
                          value={solSender}
                          onChange={(e) => setSolSender(e.target.value)}
                          placeholder="Phantom wallet pubkey"
                          className="w-full rounded-lg border border-slate-800 bg-slate-950 px-3 py-2 font-mono text-xs text-slate-100 placeholder:text-slate-600"
                        />
                      </div>
                      <div>
                        <label className="mb-1 block font-mono text-[10px] uppercase tracking-wider text-slate-400">
                          Solana Maker Recipient
                        </label>
                        <input
                          type="text"
                          required
                          value={solRecipient}
                          onChange={(e) => setSolRecipient(e.target.value)}
                          placeholder="Counterparty Base58 pubkey"
                          className="w-full rounded-lg border border-slate-800 bg-slate-950 px-3 py-2 font-mono text-xs text-slate-100 placeholder:text-slate-600"
                        />
                      </div>
                    </div>
                  )}

                  {rail === 'evm_htlc' && (
                    <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
                      <div>
                        <label className="mb-1 block font-mono text-[10px] uppercase tracking-wider text-slate-400">
                          EVM Asset
                        </label>
                        <select
                          value={sourceAsset}
                          onChange={(e) => setSourceAsset(e.target.value)}
                          className="w-full rounded-lg border border-slate-800 bg-slate-950 px-3 py-2 font-mono text-xs text-slate-100"
                        >
                          <option value="USDC">USDC (ERC-20)</option>
                          <option value="ETH">Native ETH</option>
                        </select>
                      </div>
                      <div className="sm:col-span-2">
                        <label className="mb-1 block font-mono text-[10px] uppercase tracking-wider text-slate-400">
                          EVM Counterparty Recipient (0x…)
                        </label>
                        <input
                          type="text"
                          required
                          value={evmRecipient}
                          onChange={(e) => setEvmRecipient(e.target.value)}
                          placeholder="0x… maker address (Base / Arbitrum / Ethereum)"
                          className="w-full rounded-lg border border-slate-800 bg-slate-950 px-3 py-2 font-mono text-xs text-slate-100 placeholder:text-slate-600"
                        />
                      </div>
                    </div>
                  )}
                </div>
              ) : (
                <div>
                  <label className="mb-1 block font-mono text-[10px] uppercase tracking-wider text-slate-400">
                    Lightning BOLT-11 Invoice (payment_hash &amp; sats decoded natively in Rust)
                  </label>
                  <textarea
                    required
                    rows={3}
                    value={bolt11Invoice}
                    onChange={(e) => setBolt11Invoice(e.target.value)}
                    placeholder="lnbc… / lntb… / lnbcrt…"
                    className="w-full rounded-lg border border-slate-800 bg-slate-950 px-3 py-2 font-mono text-xs text-slate-100 placeholder:text-slate-600 focus:border-amber-500 focus:outline-none"
                  />
                </div>
              )}

              <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
                <div>
                  <label className="mb-1 block font-mono text-[10px] uppercase tracking-wider text-slate-400">
                    LOVE Amount
                  </label>
                  <input
                    type="text"
                    required
                    value={amountLove}
                    onChange={(e) => setAmountLove(e.target.value)}
                    placeholder="e.g. 250"
                    className="w-full rounded-lg border border-slate-800 bg-slate-950 px-3 py-2 font-mono text-xs text-slate-100 placeholder:text-slate-600"
                  />
                </div>
                <div>
                  <label className="mb-1 block font-mono text-[10px] uppercase tracking-wider text-slate-400">
                    {rail === 'solana_htlc' || rail === 'evm_htlc'
                      ? `${sourceAsset} Base Units`
                      : 'BTC Amount (sats)'}
                  </label>
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
                          ? 'e.g. 50000000 (50 USDC)'
                          : 'e.g. 500000'
                    }
                    className="w-full rounded-lg border border-slate-800 bg-slate-950 px-3 py-2 font-mono text-xs text-slate-100 placeholder:text-slate-600"
                  />
                </div>
                <div>
                  <label className="mb-1 block font-mono text-[10px] uppercase tracking-wider text-slate-400">
                    LOVE Timeout (blocks)
                  </label>
                  <input
                    type="number"
                    min={1}
                    max={20160}
                    required
                    value={loveTimeoutBlocks}
                    onChange={(e) => setLoveTimeoutBlocks(e.target.value)}
                    className="w-full rounded-lg border border-slate-800 bg-slate-950 px-3 py-2 font-mono text-xs text-slate-100"
                  />
                </div>
              </div>

              <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                <div>
                  <label className="mb-1 block font-mono text-[10px] uppercase tracking-wider text-slate-400">
                    Newsatoshi Claim Address (ns1… / nsd1…)
                  </label>
                  <input
                    type="text"
                    required
                    value={loveClaimAddr}
                    onChange={(e) => setLoveClaimAddr(e.target.value)}
                    placeholder="ns1… or nsd1…"
                    className="w-full rounded-lg border border-slate-800 bg-slate-950 px-3 py-2 font-mono text-xs text-slate-100 placeholder:text-slate-600"
                  />
                </div>

                {rail === 'btc_onchain' ? (
                  <div className="grid grid-cols-2 gap-2">
                    <div>
                      <label className="mb-1 block font-mono text-[10px] uppercase tracking-wider text-slate-400">
                        BTC CLTV Height
                      </label>
                      <input
                        type="number"
                        min={1}
                        required
                        value={btcLocktime}
                        onChange={(e) => setBtcLocktime(e.target.value)}
                        placeholder="e.g. 920072"
                        className="w-full rounded-lg border border-slate-800 bg-slate-950 px-3 py-2 font-mono text-xs text-slate-100 placeholder:text-slate-600"
                      />
                    </div>
                    <div>
                      <label className="mb-1 block font-mono text-[10px] uppercase tracking-wider text-slate-400">
                        BTC Tip (optional)
                      </label>
                      <input
                        type="number"
                        min={1}
                        value={btcCurrentHeight}
                        onChange={(e) => setBtcCurrentHeight(e.target.value)}
                        placeholder="For margin check"
                        className="w-full rounded-lg border border-slate-800 bg-slate-950 px-3 py-2 font-mono text-xs text-slate-100 placeholder:text-slate-600"
                      />
                    </div>
                  </div>
                ) : (
                  <div>
                    <label className="mb-1 block font-mono text-[10px] uppercase tracking-wider text-slate-400">
                      Counterparty Label (optional)
                    </label>
                    <input
                      type="text"
                      value={makerLabel}
                      onChange={(e) => setMakerLabel(e.target.value)}
                      placeholder="Optional swap tag"
                      className="w-full rounded-lg border border-slate-800 bg-slate-950 px-3 py-2 font-mono text-xs text-slate-100 placeholder:text-slate-600"
                    />
                  </div>
                )}
              </div>

              {compileError && (
                <div className="rounded-lg border border-red-500/40 bg-red-500/10 px-3 py-2 text-xs text-red-200">
                  {compileError}
                </div>
              )}

              <div className="flex flex-wrap items-center gap-2.5 pt-1">
                <button
                  type="submit"
                  disabled={compiling}
                  className="inline-flex items-center gap-2 rounded-lg bg-emerald-500 px-4 py-2 text-xs font-semibold text-slate-950 hover:bg-emerald-400 cursor-pointer disabled:opacity-50"
                >
                  <ShieldCheck className="h-4 w-4" />
                  {compiling ? 'Compiling HTLC…' : 'Compile HTLC Swap Contract'}
                </button>
                {hasOperatorToken && (
                  <button
                    type="button"
                    disabled={publishing}
                    onClick={() => void handlePublish()}
                    className="inline-flex items-center gap-2 rounded-lg border border-sky-500/50 bg-sky-500/15 px-3.5 py-2 text-xs font-semibold text-sky-200 hover:bg-sky-500/25 cursor-pointer disabled:opacity-50"
                  >
                    {publishing ? 'Publishing…' : 'Publish to Swap Board'}
                  </button>
                )}
              </div>
            </form>

            {compiled && (
              <div className="mt-5 space-y-3 rounded-xl border border-emerald-500/35 bg-[#04080f] p-4">
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <div className="flex items-center gap-2">
                    <CheckCircle2 className="h-4 w-4 text-emerald-400" />
                    <span className="font-mono text-xs font-semibold text-emerald-200">
                      {compiled.swap_id} · {compiled.amount_love} LOVE ↔{' '}
                      {compiled.amount_btc_sats.toLocaleString()} sats
                    </span>
                  </div>
                  <span
                    className={`rounded-full border px-2 py-0.5 font-mono text-[10px] font-semibold ${
                      compiled.timelock_safe
                        ? 'border-emerald-500/40 bg-emerald-500/15 text-emerald-300'
                        : 'border-amber-500/40 bg-amber-500/15 text-amber-300'
                    }`}
                  >
                    {compiled.timelock_safe ? 'TIMELOCK SAFE' : 'CHECK TIMELOCK'}
                  </span>
                </div>

                <p className="text-[11px] leading-relaxed text-slate-300">{compiled.timelock_note}</p>

                {compiled.btc_htlc && (
                  <div className="space-y-2 rounded-lg border border-slate-800 bg-slate-950/90 p-3 text-xs">
                    <div className="flex flex-wrap items-center justify-between gap-2">
                      <span className="font-mono text-[10px] uppercase text-sky-300">
                        Bitcoin SegWit v0 P2WSH HTLC Address ({compiled.btc_htlc.btc_network})
                      </span>
                      <HashPill hash={compiled.btc_htlc.btc_p2wsh_address} full />
                    </div>
                    <div>
                      <div className="mb-1 flex items-center justify-between">
                        <span className="font-mono text-[10px] uppercase text-slate-400">
                          BIP-199 Witness Script (Hex)
                        </span>
                        <CopyButton value={compiled.btc_htlc.witness_script_hex} label="Copy Script" />
                      </div>
                      <code className="block break-all font-mono text-[10px] text-slate-300">
                        {compiled.btc_htlc.witness_script_hex}
                      </code>
                    </div>
                  </div>
                )}

                {compiled.bolt11 && (
                  <div className="rounded-lg border border-amber-500/30 bg-amber-500/5 p-3 font-mono text-[11px] text-amber-200">
                    Decoded BOLT-11 ({compiled.bolt11.network}): payment_hash{' '}
                    <span className="break-all text-slate-100">{compiled.bolt11.payment_hash_hex}</span>
                    {compiled.bolt11.description ? ` · "${compiled.bolt11.description}"` : ''}
                  </div>
                )}

                {compiled.solana_htlc && (
                  <div className="space-y-2 rounded-lg border border-purple-500/35 bg-purple-500/5 p-3 text-xs">
                    <div className="flex items-center justify-between">
                      <span className="font-mono text-[10px] uppercase text-purple-300">
                        Solana SHA-256 HTLC Escrow ({compiled.solana_htlc.cluster} · {compiled.solana_htlc.asset})
                      </span>
                      <CopyButton value={compiled.solana_htlc.lock_instruction_hex} label="Copy 49B Ix" />
                    </div>
                    <code className="block break-all font-mono text-[10px] text-slate-200">
                      {compiled.solana_htlc.pda_seeds_description}
                    </code>
                    <code className="block break-all font-mono text-[10px] text-purple-200">
                      Lock Ix (49B): {compiled.solana_htlc.lock_instruction_hex}
                    </code>
                    <p className="text-[11px] text-slate-400">{compiled.solana_htlc.jupiter_route_note}</p>
                  </div>
                )}

                {compiled.evm_htlc && (
                  <div className="space-y-2 rounded-lg border border-sky-500/35 bg-sky-500/5 p-3 text-xs">
                    <div className="flex flex-wrap items-center justify-between gap-2">
                      <span className="font-mono text-[10px] uppercase text-sky-300">
                        {compiled.evm_htlc.chain.toUpperCase()} (Chain {compiled.evm_htlc.chain_id}) · {compiled.evm_htlc.asset} HTLC
                      </span>
                      <CopyButton value={compiled.evm_htlc.cast_lock_command} label="Copy cast" />
                    </div>
                    <div className="flex flex-wrap items-center justify-between gap-2 font-mono text-[10px] text-slate-300">
                      <span>Contract:</span>
                      {(() => {
                        const explorerBase: Record<number, string> = {
                          8453: 'https://basescan.org/address/',
                          42161: 'https://arbiscan.io/address/',
                          10: 'https://optimistic.etherscan.io/address/',
                          137: 'https://polygonscan.com/address/',
                        };
                        const base = explorerBase[compiled.evm_htlc.chain_id];
                        return base && compiled.evm_htlc.contract_address.startsWith('0x') ? (
                          <a
                            href={`${base}${compiled.evm_htlc.contract_address}`}
                            target="_blank"
                            rel="noreferrer"
                            className="text-emerald-400 hover:underline break-all"
                          >
                            {compiled.evm_htlc.contract_address} ↗
                          </a>
                        ) : (
                          <code className="break-all text-slate-200">{compiled.evm_htlc.contract_address}</code>
                        );
                      })()}
                    </div>
                    <code className="block break-all font-mono text-[10px] text-sky-200">
                      {compiled.evm_htlc.cast_lock_command}
                    </code>
                  </div>
                )}

                <div className="space-y-1.5 rounded-lg border border-slate-800 bg-slate-950/90 p-3">
                  <div className="flex items-center justify-between">
                    <span className="font-mono text-[10px] uppercase text-emerald-400">
                      Newsatoshi L1 Lock Command ({compiled.love_network})
                    </span>
                    <CopyButton value={compiled.love_lock_command} label="Copy CLI" />
                  </div>
                  <code className="block break-all font-mono text-[11px] text-emerald-200">
                    {compiled.love_lock_command}
                  </code>
                </div>
              </div>
            )}
          </Panel>
        </div>

        {/* Live Dual-Chain HTLC Verifier + Swap Board */}
        <div className="space-y-6 xl:col-span-5">
          <Panel
            title="Live Dual-Chain HTLC Verifier"
            subtitle="Cross-checks Newsatoshi L1 (/api/lock?hash=) and Bitcoin (mempool.space UTXOs) in real time"
            icon={<SearchCheck className="h-4 w-4" />}
            accent="sky"
          >
            <form onSubmit={handleVerifyBothChains} className="space-y-3">
              <div className="grid grid-cols-1 gap-2 sm:grid-cols-3">
                <div className="sm:col-span-2">
                  <label className="mb-1 block font-mono text-[10px] uppercase text-slate-400">
                    SHA-256 Payment / Anchor Hash
                  </label>
                  <input
                    type="text"
                    required
                    value={verifyHash}
                    onChange={(e) => setVerifyHash(e.target.value)}
                    placeholder="64-hex hash to check on-chain"
                    className="w-full rounded-lg border border-slate-800 bg-slate-950 px-3 py-2 font-mono text-xs text-slate-100 placeholder:text-slate-600"
                  />
                </div>
                <div>
                  <label className="mb-1 block font-mono text-[10px] uppercase text-slate-400">
                    L1 Network
                  </label>
                  <select
                    value={verifyLoveNet}
                    onChange={(e) => setVerifyLoveNet(e.target.value as 'mainnet' | 'testnet')}
                    className="w-full rounded-lg border border-slate-800 bg-slate-950 px-2.5 py-2 font-mono text-xs text-slate-200"
                  >
                    <option value="mainnet">Mainnet</option>
                    <option value="testnet">Testnet</option>
                  </select>
                </div>
              </div>

              <div>
                <label className="mb-1 block font-mono text-[10px] uppercase text-slate-400">
                  Bitcoin P2WSH Address (optional — live mempool.space UTXO check)
                </label>
                <input
                  type="text"
                  value={verifyBtcAddr}
                  onChange={(e) => setVerifyBtcAddr(e.target.value)}
                  placeholder="bc1q… or tb1q…"
                  className="w-full rounded-lg border border-slate-800 bg-slate-950 px-3 py-2 font-mono text-xs text-slate-100 placeholder:text-slate-600"
                />
              </div>

              <div>
                <label className="mb-1 block font-mono text-[10px] uppercase text-slate-400">
                  Preimage Check (optional — verifies SHA256(preimage) == hash)
                </label>
                <input
                  type="text"
                  value={verifyPreimage}
                  onChange={(e) => setVerifyPreimage(e.target.value)}
                  placeholder="Optional 64-hex revealed preimage"
                  className="w-full rounded-lg border border-slate-800 bg-slate-950 px-3 py-2 font-mono text-xs text-slate-100 placeholder:text-slate-600"
                />
              </div>

              <button
                type="submit"
                disabled={verifying}
                className="inline-flex w-full items-center justify-center gap-2 rounded-lg bg-sky-500 px-4 py-2 text-xs font-semibold text-slate-950 hover:bg-sky-400 cursor-pointer disabled:opacity-50"
              >
                <Radio className="h-4 w-4" />
                {verifying ? 'Querying Live L1 Chains…' : 'Verify Both Chains Live'}
              </button>

              {verifyError && (
                <div className="rounded-lg border border-red-500/40 bg-red-500/10 px-3 py-2 text-xs text-red-200">
                  {verifyError}
                </div>
              )}

              {verifyResult && (
                <div className="space-y-2 rounded-lg border border-slate-800 bg-[#04080f] p-3 font-mono text-[11px]">
                  <div className="flex items-center justify-between">
                    <span className="text-slate-400">Newsatoshi ({verifyResult.love_network})</span>
                    <span
                      className={
                        verifyResult.love_confirmed
                          ? 'font-semibold text-emerald-300'
                          : verifyResult.love_found
                            ? 'text-amber-300'
                            : 'text-slate-400'
                      }
                    >
                      {verifyResult.love_confirmed
                        ? `CONFIRMED #${verifyResult.love_lock?.at.toLocaleString()} (${verifyResult.love_lock?.confirmations} confs)`
                        : verifyResult.love_found
                          ? 'IN MEMPOOL (UNCONFIRMED)'
                          : 'NO LOCK FOUND'}
                    </span>
                  </div>
                  {verifyResult.love_lock && (
                    <div className="flex items-center justify-between text-[10px] text-slate-400">
                      <span>Lock ID ({verifyResult.love_lock.state})</span>
                      <HashPill hash={verifyResult.love_lock.id} />
                    </div>
                  )}
                  {verifyResult.btc_address && (
                    <div className="flex items-center justify-between border-t border-slate-800/80 pt-2">
                      <span className="text-slate-400">Bitcoin P2WSH</span>
                      <span
                        className={
                          verifyResult.btc_confirmed
                            ? 'font-semibold text-emerald-300'
                            : verifyResult.btc_found
                              ? 'text-amber-300'
                              : 'text-slate-400'
                        }
                      >
                        {verifyResult.btc_error
                          ? verifyResult.btc_error
                          : verifyResult.btc_found
                            ? `${verifyResult.btc_total_sats.toLocaleString()} sats (${verifyResult.btc_confirmed ? 'confirmed' : 'mempool'})`
                            : '0 UTXOs (unfunded)'}
                      </span>
                    </div>
                  )}
                  {verifyResult.preimage_valid !== null && verifyResult.preimage_valid !== undefined && (
                    <div className="flex items-center justify-between border-t border-slate-800/80 pt-2">
                      <span className="text-slate-400">SHA-256 Preimage</span>
                      <span
                        className={
                          verifyResult.preimage_valid
                            ? 'font-semibold text-emerald-300'
                            : 'font-semibold text-red-400'
                        }
                      >
                        {verifyResult.preimage_valid ? 'VALID MATCH' : 'MISMATCH'}
                      </span>
                    </div>
                  )}
                </div>
              )}
            </form>
          </Panel>

          <Panel
            title="Non-Custodial Swap Board"
            subtitle="Strictly real persisted HTLC swap intents — zero fake orders or simulated liquidity"
            icon={<AlertTriangle className="h-4 w-4 text-amber-400" />}
            badge={
              <span className="font-mono text-[11px] text-slate-300">
                {swapOffers.length} active
              </span>
            }
          >
            {swapOffers.length === 0 ? (
              <div className="rounded-lg border border-dashed border-slate-800 bg-slate-950/60 px-4 py-6 text-center text-xs text-slate-400">
                No swap intents published yet. Compile any real HTLC contract on the left or run{' '}
                <code className="font-mono text-emerald-300">newsatoshi-swap htlc</code> locally.
              </div>
            ) : (
              <div className="space-y-2.5">
                {swapOffers.map((offer) => (
                  <div
                    key={offer.swap_id}
                    className="rounded-lg border border-slate-800 bg-slate-950/80 p-3 text-xs"
                  >
                    <div className="flex items-center justify-between gap-2">
                      <span className="font-mono font-semibold text-slate-100">
                        {offer.amount_love} LOVE ↔ {offer.amount_btc_sats.toLocaleString()} sats
                      </span>
                      <span className="rounded border border-slate-700 px-1.5 py-0.5 font-mono text-[10px] text-slate-300">
                        {offer.love_network} · {offer.rail}
                      </span>
                    </div>
                    <div className="mt-1.5 flex items-center justify-between text-[11px] text-slate-400">
                      <span>payment_hash</span>
                      <HashPill hash={offer.payment_hash_hex} />
                    </div>
                  </div>
                ))}
              </div>
            )}
          </Panel>
        </div>
      </div>
      )}

      {/* Section B: 1-Command AuxPoW Stratum Merge-Mining Kit + Field Meter Signer */}
      {section === 'mining' && (
      <div className="grid grid-cols-1 gap-6 lg:grid-cols-2">
        <Panel
          title="1-Command AuxPoW Stratum Merge-Mining Kit"
          subtitle="Mine Newsatoshi Mainnet (chainid = 20051) alongside Bitcoin on Bitaxe, NerdQAxe, Antminer S19/S21, or solo bitcoind"
          icon={<Pickaxe className="h-4 w-4" />}
          accent="amber"
          badge={
            <span className="rounded-full border border-amber-500/40 bg-amber-500/10 px-2.5 py-0.5 font-mono text-[10px] font-semibold text-amber-300">
              CHAIN ID 20051 · 50 LOVE/BLK
            </span>
          }
        >
          <div className="space-y-4">
            <div className="grid grid-cols-2 gap-2.5 sm:grid-cols-4">
              <div className="rounded-lg border border-slate-800 bg-slate-950/75 p-2.5">
                <div className="font-mono text-[10px] uppercase text-slate-500">Mainnet Height</div>
                <div className="mt-0.5 font-mono text-sm font-bold text-emerald-300">
                  {mainnet ? `#${mainnet.height.toLocaleString()}` : 'Offline'}
                </div>
              </div>
              <div className="rounded-lg border border-slate-800 bg-slate-950/75 p-2.5">
                <div className="font-mono text-[10px] uppercase text-slate-500">Difficulty</div>
                <div className="mt-0.5 font-mono text-sm font-bold text-slate-100">
                  {mainnet ? mainnet.difficulty.toFixed(2) : '—'}
                </div>
              </div>
              <div className="rounded-lg border border-slate-800 bg-slate-950/75 p-2.5">
                <div className="font-mono text-[10px] uppercase text-slate-500">Mainnet Locks</div>
                <div className="mt-0.5 font-mono text-sm font-bold text-sky-300">
                  {mainnet ? mainnet.locks.toLocaleString() : '—'}
                </div>
              </div>
              <div className="rounded-lg border border-slate-800 bg-slate-950/75 p-2.5">
                <div className="font-mono text-[10px] uppercase text-slate-500">Peers</div>
                <div className="mt-0.5 font-mono text-sm font-bold text-slate-100">
                  {mainnet ? mainnet.peers : '—'}
                </div>
              </div>
            </div>

            {mainnet && (() => {
              const ths = Math.max(0.001, Number(minerThs) || 1.2);
              const hashesPerBlock = Math.max(1e-9, mainnet.difficulty) * 4_294_967_296;
              const recent = mainnet.recent ?? [];
              const mergePct = recent.length > 0
                ? (recent.filter((b) => b.merge_mined).length / recent.length) * 100
                : 0;
              const obsDt = recent.length >= 2 && recent[0].time > recent[recent.length - 1].time
                ? (recent[0].time - recent[recent.length - 1].time) / (recent.length - 1)
                : 600;
              const netHs = hashesPerBlock / Math.max(1, obsDt);
              const netGhs = netHs / 1e9;
              const minerHs = ths * 1e12;
              const postAsertShare = minerHs / (netHs + minerHs);
              const postAsertLoveDay = postAsertShare * 7_200;
              return (
                <div className="rounded-lg border border-amber-500/25 bg-amber-950/15 p-3 space-y-2.5">
                  <div className="flex flex-wrap items-center justify-between gap-2">
                    <span className="font-mono text-[10px] uppercase tracking-wider text-amber-300">
                      Live AuxPoW Yield Calculator (Zero Extra Electricity)
                    </span>
                    <span className="font-mono text-[10px] text-slate-400">
                      Est. Net: {netGhs.toFixed(2)} GH/s · {mergePct.toFixed(0)}% merge-mined
                    </span>
                  </div>
                  <div className="flex flex-wrap items-center gap-1.5">
                    {[
                      { label: 'Bitaxe (1.2 TH/s)', v: '1.2' },
                      { label: 'NerdQAxe++ (4.8)', v: '4.8' },
                      { label: 'S19 (110)', v: '110' },
                      { label: 'S21 (200)', v: '200' },
                    ].map((preset) => (
                      <button
                        key={preset.v}
                        type="button"
                        onClick={() => setMinerThs(preset.v)}
                        className={`rounded border px-2 py-0.5 font-mono text-[10px] transition cursor-pointer ${
                          minerThs === preset.v
                            ? 'border-amber-400/70 bg-amber-500/20 text-amber-200'
                            : 'border-slate-700 bg-slate-900/70 text-slate-400 hover:text-slate-200'
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
                      className="w-20 rounded border border-slate-700 bg-slate-950 px-2 py-0.5 font-mono text-[11px] text-slate-100"
                    />
                    <span className="font-mono text-[10px] text-slate-400">TH/s</span>
                  </div>
                  <div className="grid grid-cols-2 gap-2 text-[11px]">
                    <div className="rounded border border-slate-800/90 bg-slate-950/75 px-2.5 py-1.5">
                      <div className="font-mono text-[9px] uppercase text-slate-500">Post-ASERT Share / 7,200 LOVE/Day</div>
                      <div className="font-mono text-xs font-bold text-amber-300">
                        {postAsertLoveDay.toLocaleString(undefined, { maximumFractionDigits: 1 })} LOVE/day ({(postAsertShare * 100).toFixed(2)}%)
                      </div>
                    </div>
                    <div className="rounded border border-slate-800/90 bg-slate-950/75 px-2.5 py-1.5">
                      <div className="font-mono text-[9px] uppercase text-slate-500">Supply Issued / 21M Cap</div>
                      <div className="font-mono text-xs font-bold text-emerald-300">
                        {Math.round(mainnet.issued / 1e8).toLocaleString()} LOVE ({((mainnet.issued / 1e8 / 21_000_000) * 100).toFixed(3)}%)
                      </div>
                    </div>
                  </div>
                  <div className="text-[10px] text-slate-400">
                    Steady-state share of 144 blocks/day × 50 LOVE (7,200 LOVE/day global emission) after ASERT retargeting against live Mainnet hashrate ({netGhs.toFixed(2)} GH/s).
                  </div>
                </div>
              );
            })()}

            <div className="rounded-lg border border-slate-800 bg-[#04080f] p-3">
              <div className="mb-1.5 flex items-center justify-between">
                <span className="inline-flex items-center gap-1.5 font-mono text-[10px] uppercase text-amber-300">
                  <Terminal className="h-3.5 w-3.5" />
                  1-Command Docker Launch (newsatoshi-0.1.0 + Stratum v1 :3333)
                </span>
                <CopyButton value={dockerQuickstart} label="Copy" />
              </div>
              <pre className="overflow-x-auto font-mono text-[11px] leading-relaxed text-slate-200">
                {dockerQuickstart}
              </pre>
            </div>

            <div className="rounded-lg border border-slate-800/80 bg-slate-950/70 px-3 py-2.5 text-[11px] text-slate-300">
              Point any <strong className="text-slate-100">Bitaxe / NerdQAxe / Antminer</strong> at{' '}
              <code className="font-mono text-emerald-300">stratum+tcp://&lt;HOST&gt;:3333</code>{' '}
              (BIP-320 version rolling supported; live status on{' '}
              <code className="font-mono text-sky-300">:3334/status</code>). Pass{' '}
              <code className="font-mono text-amber-200">--btc-rpc-url</code> to simultaneously
              merge-mine Bitcoin and Newsatoshi on the same hashes.
            </div>
          </div>
        </Panel>

        <Panel
          title="Zero-Disk-Secret Field Hardware Meter Daemon"
          subtitle="Signs real solar inverter / smart-meter kWh deltas offline with NIST FIPS 205 SLH-DSA and posts directly to /api/telemetry"
          icon={<Cpu className="h-4 w-4" />}
          accent="sky"
          badge={
            <span className="rounded-full border border-sky-500/40 bg-sky-500/10 px-2.5 py-0.5 font-mono text-[10px] font-semibold text-sky-300">
              FIPS 205 SLH-DSA · 0 DISK SECRETS
            </span>
          }
        >
          <div className="space-y-4">
            <div className="grid grid-cols-2 gap-2 text-[11px]">
              <div className="rounded-lg border border-slate-800 bg-slate-950/75 p-2.5">
                <div className="font-mono text-[10px] uppercase text-slate-500">Supported Meters</div>
                <div className="mt-0.5 font-mono text-slate-200">
                  Shelly EM · Enphase IQ · Home Assistant · Generic JSON
                </div>
              </div>
              <div className="rounded-lg border border-slate-800 bg-slate-950/75 p-2.5">
                <div className="font-mono text-[10px] uppercase text-slate-500">Authorized Hardware Root</div>
                <div className="mt-1">
                  <HashPill hash={state?.keys.hardware_merkle_root ?? ''} />
                </div>
              </div>
            </div>

            <div className="rounded-lg border border-slate-800 bg-[#04080f] p-3">
              <div className="mb-1.5 flex items-center justify-between">
                <span className="inline-flex items-center gap-1.5 font-mono text-[10px] uppercase text-sky-300">
                  <Terminal className="h-3.5 w-3.5" />
                  Run Real Hardware Meter Attestation (Keychain / Env Seeds Only)
                </span>
                <CopyButton value={meterDaemonCmd} label="Copy" />
              </div>
              <pre className="overflow-x-auto font-mono text-[11px] leading-relaxed text-slate-200">
                {meterDaemonCmd}
              </pre>
            </div>

            <div className="rounded-lg border border-slate-800/80 bg-slate-950/70 px-3 py-2.5 text-[11px] text-slate-300">
              First poll records the physical cumulative baseline with{' '}
              <strong className="text-slate-100">0 synthetic emissions</strong>. Every subsequent
              whole-kWh production delta is signed offline with your cold-ceremony SLH-DSA key and
              verified by this <code className="font-mono text-sky-300">--verify-only</code> server
              into a live Winterfell STARK proof.
            </div>
          </div>
        </Panel>
      </div>
      )}
    </div>
  );
}
